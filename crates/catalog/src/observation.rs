use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use serde::Deserialize;

use crate::{
    CatalogError, DomainType, NamedNativeCatalog, NativeCatalog, NativeColumn, NativeRelation,
    NativeRelationName, NativeType, TypeKind,
    relation::{
        column_generation, column_identity, relation_kind, relation_persistence, type_kind,
    },
};

pub const CATALOG_REQUEST_MAX_BYTES: usize = 1024 * 1024;
pub const CATALOG_REQUEST_MAX_PAIRS: usize = 4096;
pub const CATALOG_RESPONSE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Identity of immutable facts observed by one native backend. This is neither
/// a live publication fence nor a promise that a cached statement can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCatalogStamp {
    cluster_id: [u8; 16],
    database_oid: u32,
    backend_id: u64,
    generation: u64,
    local_generation: u64,
}

impl NativeCatalogStamp {
    pub fn cluster_id(self) -> [u8; 16] {
        self.cluster_id
    }

    pub fn database_oid(self) -> u32 {
        self.database_oid
    }

    pub fn backend_id(self) -> u64 {
        self.backend_id
    }

    pub fn generation(self) -> u64 {
        self.generation
    }

    pub fn local_generation(self) -> u64 {
        self.local_generation
    }
}

/// Complete named facts and their publication identity from a one-shot read.
/// Only decoding constructs this representation. A caller still has to prove
/// the source connection and complete request outcome; decoding is not a lease.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct CatalogObservation {
    stamp: NativeCatalogStamp,
    named: NamedNativeCatalog,
}

impl CatalogObservation {
    pub fn stamp(&self) -> NativeCatalogStamp {
        self.stamp
    }

    pub fn named_catalog(&self) -> &NamedNativeCatalog {
        &self.named
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogObservationError {
    #[error("native catalog response reaches the 64 MiB limit")]
    ResponseTooLarge,
    #[error("invalid native catalog response: {0}")]
    InvalidResponse(&'static str),
    #[error("native catalog JSON decoding failed: {0}")]
    Decode(#[from] serde_json::Error),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    protocol: u32,
    stamp: Stamp,
    relation_oids: Vec<u32>,
    relations: Vec<Relation>,
    types: Vec<Type>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    cluster_id: String,
    database_oid: u32,
    backend_id: u64,
    generation: u64,
    local_generation: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Relation {
    oid: u32,
    schema_oid: u32,
    schema_name: String,
    name: String,
    kind: u8,
    persistence: u8,
    is_partition: bool,
    columns: Vec<Column>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    attribute_number: i16,
    name: String,
    declared_type_oid: u32,
    type_modifier: i32,
    array_dimensions: i16,
    not_null_constraint: bool,
    has_expression: bool,
    identity: u8,
    generation: u8,
    collation_oid: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Type {
    oid: u32,
    schema_oid: u32,
    schema_name: String,
    name: String,
    kind: u8,
    category: u8,
    element_type_oid: u32,
    base_type_oid: u32,
    base_type_modifier: i32,
    not_null_constraint: bool,
    array_dimensions: i32,
}

fn invalid(message: &'static str) -> CatalogObservationError {
    CatalogObservationError::InvalidResponse(message)
}

fn check_name(name: &str) -> Result<(), CatalogObservationError> {
    if name.is_empty() || name.len() >= 64 || name.contains('\0') {
        Err(invalid("invalid native catalog name"))
    } else {
        Ok(())
    }
}

fn check_oid(oid: u32) -> Result<(), CatalogObservationError> {
    if oid == 0 {
        Err(invalid("zero required object identity"))
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct Schemas {
    names: BTreeMap<u32, String>,
    oids: BTreeMap<String, u32>,
}

fn check_schema(
    schemas: &mut Schemas,
    oid: u32,
    name: &str,
) -> Result<(), CatalogObservationError> {
    check_oid(oid)?;
    check_name(name)?;
    match schemas.names.entry(oid) {
        Entry::Vacant(entry) => {
            entry.insert(name.to_owned());
        }
        Entry::Occupied(entry) if entry.get() != name => {
            return Err(invalid("one namespace OID has different names"));
        }
        Entry::Occupied(_) => {}
    }
    match schemas.oids.entry(name.to_owned()) {
        Entry::Vacant(entry) => {
            entry.insert(oid);
        }
        Entry::Occupied(entry) if *entry.get() != oid => {
            return Err(invalid("one namespace name has different OIDs"));
        }
        Entry::Occupied(_) => {}
    }
    Ok(())
}

impl Stamp {
    fn decode(self) -> Result<NativeCatalogStamp, CatalogObservationError> {
        check_oid(self.database_oid)?;
        if [self.backend_id, self.generation, self.local_generation]
            .iter()
            .any(|value| *value == 0 || *value > i64::MAX as u64)
        {
            return Err(invalid("invalid publication counter"));
        }
        let text = self.cluster_id.as_bytes();
        if text.len() != 32
            || text
                .iter()
                .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
        {
            return Err(invalid("invalid server incarnation"));
        }
        let mut cluster_id = [0; 16];
        let nibble = |byte: u8| {
            if byte.is_ascii_digit() {
                byte - b'0'
            } else {
                byte - b'a' + 10
            }
        };
        for (output, pair) in cluster_id.iter_mut().zip(text.chunks_exact(2)) {
            *output = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        if cluster_id == [0; 16] {
            return Err(invalid("zero server incarnation"));
        }
        Ok(NativeCatalogStamp {
            cluster_id,
            database_oid: self.database_oid,
            backend_id: self.backend_id,
            generation: self.generation,
            local_generation: self.local_generation,
        })
    }
}

/// Decode a complete protocol-1 observation, preserving exact request order.
/// Unknown/duplicate fields, codes, object identities, missing/extra facts and
/// cyclic domain ancestry fail. Placeholder GUC echoes are not observations.
/// This pure function proves representation only, not the originating backend,
/// completion sequence, supported owner state or later statement validity.
pub fn decode_catalog_observation(
    response: &str,
    requested: &[NativeRelationName<'_>],
) -> Result<CatalogObservation, CatalogObservationError> {
    if response.len() >= CATALOG_RESPONSE_MAX_BYTES {
        return Err(CatalogObservationError::ResponseTooLarge);
    }
    let response: Response = serde_json::from_str(response)?;
    if response.protocol != 1 {
        return Err(invalid("unsupported protocol version"));
    }
    if response.relation_oids.len() != requested.len() {
        return Err(invalid("request and resolved relation counts differ"));
    }
    let stamp = response.stamp.decode()?;
    let mut catalog = NativeCatalog::default();
    let mut schemas = Schemas::default();
    let mut relation_names = BTreeSet::new();
    for relation in response.relations {
        check_oid(relation.oid)?;
        check_schema(&mut schemas, relation.schema_oid, &relation.schema_name)?;
        check_name(&relation.name)?;
        if !relation_names.insert((relation.schema_oid, relation.name.clone())) {
            return Err(invalid("duplicate native relation name"));
        }
        let mut columns = Vec::with_capacity(relation.columns.len());
        let mut previous = 0;
        let mut names = BTreeSet::new();
        for column in relation.columns {
            if column.attribute_number <= previous {
                return Err(invalid(
                    "nonpositive, duplicate or unordered attribute number",
                ));
            }
            previous = column.attribute_number;
            check_name(&column.name)?;
            check_oid(column.declared_type_oid)?;
            if !names.insert(column.name.clone()) {
                return Err(invalid("duplicate native column name"));
            }
            columns.push(NativeColumn {
                attribute_number: column.attribute_number,
                name: column.name,
                declared_type_oid: column.declared_type_oid,
                type_modifier: column.type_modifier,
                array_dimensions: column.array_dimensions,
                not_null_constraint: column.not_null_constraint,
                has_expression: column.has_expression,
                identity: column_identity(column.identity)?,
                generation: column_generation(column.generation)?,
                collation_oid: (column.collation_oid != 0).then_some(column.collation_oid),
            });
        }
        let fact = NativeRelation {
            oid: relation.oid,
            schema_oid: relation.schema_oid,
            schema_name: relation.schema_name,
            name: relation.name,
            kind: relation_kind(relation.kind)?,
            persistence: relation_persistence(relation.persistence)?,
            is_partition: relation.is_partition,
            columns,
        };
        if catalog.relations.insert(fact.oid, fact).is_some() {
            return Err(invalid("duplicate native relation OID"));
        }
    }
    let mut resolved = BTreeSet::new();
    for (name, oid) in requested.iter().zip(&response.relation_oids) {
        let relation = catalog
            .relations
            .get(oid)
            .ok_or_else(|| invalid("resolved relation is missing"))?;
        if relation.schema_name != name.schema_name || relation.name != name.relation_name {
            return Err(invalid(
                "resolved relation does not match its literal request",
            ));
        }
        resolved.insert(*oid);
    }
    if resolved.len() != catalog.relations.len() {
        return Err(invalid("unrequested relation facts"));
    }
    let mut type_names = BTreeSet::new();
    for native_type in response.types {
        check_oid(native_type.oid)?;
        check_schema(
            &mut schemas,
            native_type.schema_oid,
            &native_type.schema_name,
        )?;
        check_name(&native_type.name)?;
        if !type_names.insert((native_type.schema_oid, native_type.name.clone())) {
            return Err(invalid("duplicate native type name"));
        }
        let kind = type_kind(native_type.kind)?;
        let domain = if kind == TypeKind::Domain {
            check_oid(native_type.base_type_oid)?;
            Some(DomainType {
                base_type_oid: native_type.base_type_oid,
                base_type_modifier: native_type.base_type_modifier,
                not_null_constraint: native_type.not_null_constraint,
                array_dimensions: native_type.array_dimensions,
            })
        } else {
            None
        };
        let fact = NativeType {
            oid: native_type.oid,
            schema_oid: native_type.schema_oid,
            schema_name: native_type.schema_name,
            name: native_type.name,
            kind,
            category: native_type.category,
            element_type_oid: (native_type.element_type_oid != 0)
                .then_some(native_type.element_type_oid),
            domain,
        };
        if catalog.types.insert(fact.oid, fact).is_some() {
            return Err(invalid("duplicate native type OID"));
        }
    }
    let mut needed = BTreeSet::new();
    for relation in catalog.relations.values() {
        for column in &relation.columns {
            let mut oid = column.declared_type_oid;
            let mut path = BTreeSet::new();
            loop {
                if needed.contains(&oid) {
                    break;
                }
                if !path.insert(oid) {
                    return Err(invalid("cyclic domain ancestry"));
                }
                let native_type = catalog
                    .types
                    .get(&oid)
                    .ok_or(CatalogError::MissingType { oid })?;
                let Some(domain) = native_type.domain else {
                    break;
                };
                oid = domain.base_type_oid;
            }
            needed.extend(path);
        }
    }
    if needed.len() != catalog.types.len() {
        return Err(invalid("unrequested type facts"));
    }
    Ok(CatalogObservation {
        stamp,
        named: NamedNativeCatalog {
            catalog,
            relation_oids: response.relation_oids,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = r#"{"protocol":1,"stamp":{"cluster_id":"0102030405060708090a0b0c0d0e0f10","database_oid":42,"backend_id":1,"generation":2,"local_generation":3},"relation_oids":[],"relations":[],"types":[]}"#;

    #[test]
    fn header_is_an_observation_and_a_placeholder_echo_is_not() {
        let result = decode_catalog_observation(HEADER, &[]).unwrap();
        assert_eq!(result.stamp().generation(), 2);
        assert_eq!(result.stamp().cluster_id()[15], 16);
        assert!(result.named_catalog().catalog().relations.is_empty());
        assert!(decode_catalog_observation(r#"[["public","items"]]"#, &[]).is_err());
    }

    #[test]
    fn version_duplicate_unknown_fields_and_invalid_identity_fail() {
        for response in [
            HEADER.replace("\"protocol\":1", "\"protocol\":2"),
            HEADER.replace("\"protocol\":1", "\"protocol\":1,\"protocol\":1"),
            HEADER.replace(
                "\"database_oid\":42",
                "\"database_oid\":42,\"database_oid\":42",
            ),
            HEADER.replace("\"backend_id\":1", "\"backend_id\":0"),
            HEADER.replace("\"generation\":2", "\"generation\":9223372036854775808"),
            HEADER.replace(
                "0102030405060708090a0b0c0d0e0f10",
                "0102030405060708090A0B0C0D0E0F10",
            ),
            HEADER.replace("\"types\":[]", "\"types\":[],\"lease\":true"),
        ] {
            assert!(decode_catalog_observation(&response, &[]).is_err());
        }
    }

    fn domain_response() -> serde_json::Value {
        let mut response: serde_json::Value = serde_json::from_str(HEADER).unwrap();
        response["relation_oids"] = serde_json::json!([100, 100]);
        response["relations"] = serde_json::json!([{
            "oid":100,"schema_oid":2200,"schema_name":"public","name":"items",
            "kind":114,"persistence":112,"is_partition":false,"columns":[{
                "attribute_number":1,"name":"id","declared_type_oid":200,
                "type_modifier":-1,"array_dimensions":0,"not_null_constraint":false,
                "has_expression":false,"identity":0,"generation":0,"collation_oid":0
            }, {
                "attribute_number":3,"name":"other","declared_type_oid":23,
                "type_modifier":-1,"array_dimensions":0,"not_null_constraint":true,
                "has_expression":false,"identity":0,"generation":0,"collation_oid":0
            }]
        }]);
        response["types"] = serde_json::json!([{
            "oid":200,"schema_oid":2200,"schema_name":"public","name":"identifier",
            "kind":100,"category":78,"element_type_oid":0,"base_type_oid":201,
            "base_type_modifier":-1,"not_null_constraint":true,"array_dimensions":0
        }, {
            "oid":201,"schema_oid":2200,"schema_name":"public","name":"inner_identifier",
            "kind":100,"category":78,"element_type_oid":0,"base_type_oid":23,
            "base_type_modifier":-1,"not_null_constraint":false,"array_dimensions":0
        }, {
            "oid":23,"schema_oid":11,"schema_name":"pg_catalog","name":"int4",
            "kind":98,"category":78,"element_type_oid":0,"base_type_oid":0,
            "base_type_modifier":-1,"not_null_constraint":false,"array_dimensions":0
        }]);
        response
    }

    fn domain_names() -> [NativeRelationName<'static>; 2] {
        [NativeRelationName {
            schema_name: "public",
            relation_name: "items",
        }; 2]
    }

    #[test]
    fn repeated_requests_dropped_column_gaps_and_complete_domain_chains_are_preserved() {
        let observation =
            decode_catalog_observation(&domain_response().to_string(), &domain_names()).unwrap();
        let named = observation.named_catalog();
        assert_eq!(named.relation_oids(), &[100, 100]);
        let catalog = named.catalog();
        assert_eq!(catalog.relations.len(), 1);
        assert_eq!(catalog.types.len(), 3);
        let columns = &catalog.relations[&100].columns;
        assert_eq!(
            columns
                .iter()
                .map(|column| column.attribute_number)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(columns[0].declared_type_oid, 200);
        assert!(!columns[0].not_null_constraint);
        assert!(catalog.types[&200].domain.unwrap().not_null_constraint);
        assert_eq!(catalog.types[&200].domain.unwrap().base_type_oid, 201);
    }

    #[test]
    fn missing_extra_and_cyclic_domain_facts_cannot_form_an_observation() {
        let mut missing = domain_response();
        missing["types"].as_array_mut().unwrap().pop();
        let mut extra = domain_response();
        let mut unrelated = extra["types"][2].clone();
        unrelated["oid"] = 24.into();
        unrelated["name"] = "unrequested".into();
        extra["types"].as_array_mut().unwrap().push(unrelated);
        let mut cycle = domain_response();
        cycle["types"][1]["base_type_oid"] = 200.into();
        for response in [missing, extra, cycle] {
            assert!(decode_catalog_observation(&response.to_string(), &domain_names()).is_err());
        }
    }

    #[test]
    fn contradictory_names_oids_columns_and_codes_fail_the_complete_response() {
        for (path, value) in [
            (
                vec!["types", "0", "schema_name"],
                serde_json::json!("other"),
            ),
            (vec!["types", "0", "schema_oid"], serde_json::json!(2201)),
            (vec!["types", "1", "name"], serde_json::json!("identifier")),
            (
                vec!["relations", "0", "columns", "1", "attribute_number"],
                serde_json::json!(1),
            ),
            (
                vec!["relations", "0", "columns", "1", "name"],
                serde_json::json!("id"),
            ),
            (
                vec!["relations", "0", "columns", "0", "identity"],
                serde_json::json!(255),
            ),
            (vec!["relations", "0", "kind"], serde_json::json!(255)),
            (vec!["types", "0", "kind"], serde_json::json!(255)),
            (vec!["relation_oids", "1"], serde_json::json!(0)),
        ] {
            let mut response = domain_response();
            let mut field = &mut response;
            for part in path {
                field = if let Ok(index) = part.parse::<usize>() {
                    &mut field[index]
                } else {
                    &mut field[part]
                };
            }
            *field = value;
            assert!(decode_catalog_observation(&response.to_string(), &domain_names()).is_err());
        }
        let mut wrong_name = domain_names();
        wrong_name[1].relation_name = "Items";
        assert!(decode_catalog_observation(&domain_response().to_string(), &wrong_name).is_err());
    }
}
