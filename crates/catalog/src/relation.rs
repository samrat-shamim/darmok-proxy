use std::collections::{BTreeMap, btree_map::Entry};

use thiserror::Error;
use tokio_postgres::{GenericClient, Row, types::Type};

/// Relations and their declared types from one PostgreSQL catalog query.
///
/// OIDs identify objects only within the connected database. The maps contain
/// each requested relation once and each declared type/domain ancestor once.
/// This is a statement snapshot, not a cache entry or an execution lease.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativeCatalog {
    pub relations: BTreeMap<u32, NativeRelation>,
    pub types: BTreeMap<u32, NativeType>,
}

/// Literal native names, already separated into schema and relation parts.
///
/// Neither part is SQL, a logical database alias, or a search-path expression.
/// Case, dots, quoting characters and Unicode spelling are significant. In
/// particular, `pg_temp` does not select the connection's temporary namespace;
/// use that namespace's actual catalog name when requesting a temporary object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeRelationName<'a> {
    pub schema_name: &'a str,
    pub relation_name: &'a str,
}

/// Named requests and their native definitions from the same statement snapshot.
///
/// The OID list preserves input order and repeated requests. Catalog maps
/// deduplicate relations and types. This result is not an execution lease.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamedNativeCatalog {
    catalog: NativeCatalog,
    relation_oids: Vec<u32>,
}

impl NamedNativeCatalog {
    pub fn catalog(&self) -> &NativeCatalog {
        &self.catalog
    }

    /// One resolved OID per request, in the caller's original order.
    pub fn relation_oids(&self) -> &[u32] {
        &self.relation_oids
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeRelation {
    pub oid: u32,
    pub schema_oid: u32,
    pub schema_name: String,
    pub name: String,
    pub kind: RelationKind,
    pub persistence: RelationPersistence,
    pub is_partition: bool,
    /// Active user columns in attribute-number order. Dropped columns leave
    /// gaps; system attributes have negative numbers and are excluded.
    pub columns: Vec<NativeColumn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Table,
    Index,
    Sequence,
    Toast,
    View,
    MaterializedView,
    Composite,
    ForeignTable,
    PartitionedTable,
    PartitionedIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationPersistence {
    Permanent,
    Unlogged,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeColumn {
    pub attribute_number: i16,
    pub name: String,
    /// The declared type's OID, including domains. RowDescription may instead
    /// report a domain's base type; these two identities are different facts.
    pub declared_type_oid: u32,
    /// PostgreSQL's raw, type-specific `atttypmod`; -1 is meaningful.
    pub type_modifier: i32,
    /// Declared `attndims`, not an enforced bound on stored array dimensions.
    pub array_dimensions: i16,
    /// Raw `attnotnull`. In PostgreSQL 18 this can describe an unvalidated
    /// constraint. It is never a proof that a projected query column is nonnull.
    pub not_null_constraint: bool,
    /// A default or generation expression exists. Identity and inherited
    /// domain defaults are separate facts; no expression SQL is inferred.
    pub has_expression: bool,
    pub identity: ColumnIdentity,
    pub generation: ColumnGeneration,
    /// None means PostgreSQL reports InvalidOid, not a MySQL collation choice.
    pub collation_oid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnIdentity {
    None,
    Always,
    ByDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnGeneration {
    None,
    Stored,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeType {
    pub oid: u32,
    pub schema_oid: u32,
    pub schema_name: String,
    pub name: String,
    pub kind: TypeKind,
    /// PostgreSQL's category code, including user-defined categories. It does
    /// not establish which codecs, comparisons or MySQL types are supported.
    pub category: u8,
    /// A subscripting element type where PostgreSQL supplies one. This alone
    /// does not establish that the type is a standard PostgreSQL array.
    pub element_type_oid: Option<u32>,
    pub domain: Option<DomainType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Base,
    Composite,
    Domain,
    Enum,
    Pseudo,
    Range,
    Multirange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainType {
    /// Immediate base type, which can itself be a domain. Its descriptor is
    /// included in the same snapshot, transitively down to a nondomain type.
    pub base_type_oid: u32,
    pub base_type_modifier: i32,
    pub not_null_constraint: bool,
    pub array_dimensions: i32,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("PostgreSQL catalog query or decoding failed")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("PostgreSQL relation OID {oid} does not exist in this catalog snapshot")]
    MissingRelation { oid: u32 },
    #[error(
        "PostgreSQL relation {schema_name:?}.{relation_name:?} at request index {index} does not exist in this catalog snapshot"
    )]
    MissingNamedRelation {
        /// Zero-based index of the first missing request.
        index: usize,
        schema_name: String,
        relation_name: String,
    },
    #[error("unknown PostgreSQL catalog code {code} in {field}")]
    UnknownCode { field: &'static str, code: i8 },
    #[error("PostgreSQL type OID {oid} is missing from the catalog snapshot")]
    MissingType { oid: u32 },
}

/// Read fresh relation facts by database-local OID in one parameterized query.
///
/// Repeated OIDs are deduplicated. A missing relation fails the complete read;
/// no partial result is returned. An empty request performs no database I/O.
/// Passing a transaction makes its own DDL visible, but does not by itself pin
/// these facts against later DDL. Callers must not cache/reuse this snapshot as
/// a statement-validity guarantee.
pub async fn read_native_relations<C>(
    client: &C,
    relation_oids: &[u32],
) -> Result<NativeCatalog, CatalogError>
where
    C: GenericClient + Sync,
{
    if relation_oids.is_empty() {
        return Ok(NativeCatalog::default());
    }
    // Supplying the known parameter type lets the connector send Parse, Bind,
    // Describe and Execute together instead of preparing in a separate trip.
    let rows = client
        .query_typed(RELATION_SQL, &[(&relation_oids, Type::OID_ARRAY)])
        .await?;
    let catalog = decode_catalog(rows)?;
    for &oid in relation_oids {
        if !catalog.relations.contains_key(&oid) {
            return Err(CatalogError::MissingRelation { oid });
        }
    }
    Ok(catalog)
}

/// Resolve literal native schema/relation pairs and read their definitions in
/// one typed query. Name lookup and all facts share the caller's SQL snapshot.
///
/// A missing pair fails the complete read with its input index; no partial
/// catalog is returned. Empty input performs no database I/O. Names are not
/// folded, parsed, normalized, truncated, or resolved through `search_path`.
/// Snapshot freshness does not protect later statement execution against DDL.
pub async fn read_native_named_relations<C>(
    client: &C,
    relation_names: &[NativeRelationName<'_>],
) -> Result<NamedNativeCatalog, CatalogError>
where
    C: GenericClient + Sync,
{
    if relation_names.is_empty() {
        return Ok(NamedNativeCatalog::default());
    }
    let mut schemas = Vec::with_capacity(relation_names.len());
    let mut names = Vec::with_capacity(relation_names.len());
    for relation in relation_names {
        schemas.push(relation.schema_name);
        names.push(relation.relation_name);
    }
    let rows = client
        .query_typed(
            NAMED_RELATION_SQL,
            &[(&schemas, Type::TEXT_ARRAY), (&names, Type::TEXT_ARRAY)],
        )
        .await?;
    let catalog = decode_catalog(rows)?;
    let mut relation_oids = Vec::with_capacity(relation_names.len());
    {
        // Borrow returned names instead of cloning them or scanning the entire
        // catalog once per request. Drop this index before moving the catalog.
        let by_name: BTreeMap<_, _> = catalog
            .relations
            .values()
            .map(|relation| {
                (
                    NativeRelationName {
                        schema_name: &relation.schema_name,
                        relation_name: &relation.name,
                    },
                    relation.oid,
                )
            })
            .collect();
        for (index, name) in relation_names.iter().enumerate() {
            let Some(&oid) = by_name.get(name) else {
                return Err(CatalogError::MissingNamedRelation {
                    index,
                    schema_name: name.schema_name.to_owned(),
                    relation_name: name.relation_name.to_owned(),
                });
            };
            relation_oids.push(oid);
        }
    }
    Ok(NamedNativeCatalog {
        catalog,
        relation_oids,
    })
}

fn decode_catalog(rows: Vec<Row>) -> Result<NativeCatalog, CatalogError> {
    let mut catalog = NativeCatalog::default();
    for row in rows {
        let oid: u32 = row.try_get(0)?;
        let relation = match catalog.relations.entry(oid) {
            Entry::Vacant(entry) => entry.insert(relation_from_row(&row)?),
            Entry::Occupied(entry) => entry.into_mut(),
        };
        if let Some(attribute_number) = row.try_get::<_, Option<i16>>(7)? {
            // Domain ancestors produce several rows per column. Ordering by
            // attribute number means the column itself is decoded only once.
            if relation.columns.last().map(|c| c.attribute_number) != Some(attribute_number) {
                relation
                    .columns
                    .push(column_from_row(&row, attribute_number)?);
            }
            let type_oid: u32 = row.try_get(17)?;
            if let Entry::Vacant(entry) = catalog.types.entry(type_oid) {
                entry.insert(type_from_row(&row, type_oid)?);
            }
        }
    }
    for relation in catalog.relations.values() {
        for column in &relation.columns {
            require_type(&catalog, column.declared_type_oid)?;
        }
    }
    for native_type in catalog.types.values() {
        if let Some(domain) = native_type.domain {
            require_type(&catalog, domain.base_type_oid)?;
        }
    }
    Ok(catalog)
}

fn require_type(catalog: &NativeCatalog, oid: u32) -> Result<(), CatalogError> {
    if catalog.types.contains_key(&oid) {
        Ok(())
    } else {
        Err(CatalogError::MissingType { oid })
    }
}

fn relation_from_row(row: &Row) -> Result<NativeRelation, CatalogError> {
    let code = row.try_get(4)?;
    let kind = match code as u8 {
        b'r' => RelationKind::Table,
        b'i' => RelationKind::Index,
        b'S' => RelationKind::Sequence,
        b't' => RelationKind::Toast,
        b'v' => RelationKind::View,
        b'm' => RelationKind::MaterializedView,
        b'c' => RelationKind::Composite,
        b'f' => RelationKind::ForeignTable,
        b'p' => RelationKind::PartitionedTable,
        b'I' => RelationKind::PartitionedIndex,
        _ => {
            return Err(CatalogError::UnknownCode {
                field: "pg_class.relkind",
                code,
            });
        }
    };
    let code = row.try_get(5)?;
    let persistence = match code as u8 {
        b'p' => RelationPersistence::Permanent,
        b'u' => RelationPersistence::Unlogged,
        b't' => RelationPersistence::Temporary,
        _ => {
            return Err(CatalogError::UnknownCode {
                field: "pg_class.relpersistence",
                code,
            });
        }
    };
    Ok(NativeRelation {
        oid: row.try_get(0)?,
        schema_oid: row.try_get(1)?,
        schema_name: row.try_get(2)?,
        name: row.try_get(3)?,
        kind,
        persistence,
        is_partition: row.try_get(6)?,
        columns: Vec::new(),
    })
}

fn column_from_row(row: &Row, attribute_number: i16) -> Result<NativeColumn, CatalogError> {
    let code = row.try_get(14)?;
    let identity = match code as u8 {
        0 => ColumnIdentity::None,
        b'a' => ColumnIdentity::Always,
        b'd' => ColumnIdentity::ByDefault,
        _ => {
            return Err(CatalogError::UnknownCode {
                field: "pg_attribute.attidentity",
                code,
            });
        }
    };
    let code = row.try_get(15)?;
    let generation = match code as u8 {
        0 => ColumnGeneration::None,
        b's' => ColumnGeneration::Stored,
        b'v' => ColumnGeneration::Virtual,
        _ => {
            return Err(CatalogError::UnknownCode {
                field: "pg_attribute.attgenerated",
                code,
            });
        }
    };
    Ok(NativeColumn {
        attribute_number,
        name: row.try_get(8)?,
        declared_type_oid: row.try_get(9)?,
        type_modifier: row.try_get(10)?,
        array_dimensions: row.try_get(11)?,
        not_null_constraint: row.try_get(12)?,
        has_expression: row.try_get(13)?,
        identity,
        generation,
        collation_oid: valid_oid(row.try_get(16)?),
    })
}

fn type_from_row(row: &Row, oid: u32) -> Result<NativeType, CatalogError> {
    let code = row.try_get(21)?;
    let kind = match code as u8 {
        b'b' => TypeKind::Base,
        b'c' => TypeKind::Composite,
        b'd' => TypeKind::Domain,
        b'e' => TypeKind::Enum,
        b'p' => TypeKind::Pseudo,
        b'r' => TypeKind::Range,
        b'm' => TypeKind::Multirange,
        _ => {
            return Err(CatalogError::UnknownCode {
                field: "pg_type.typtype",
                code,
            });
        }
    };
    let domain = if kind == TypeKind::Domain {
        Some(DomainType {
            base_type_oid: row.try_get(24)?,
            base_type_modifier: row.try_get(25)?,
            not_null_constraint: row.try_get(26)?,
            array_dimensions: row.try_get(27)?,
        })
    } else {
        None
    };
    Ok(NativeType {
        oid,
        schema_oid: row.try_get(18)?,
        schema_name: row.try_get(19)?,
        name: row.try_get(20)?,
        kind,
        category: row.try_get::<_, i8>(22)? as u8,
        element_type_oid: valid_oid(row.try_get(23)?),
        domain,
    })
}

fn valid_oid(oid: u32) -> Option<u32> {
    if oid == 0 { None } else { Some(oid) }
}

// One catalog SELECT supplies relation/column facts and the complete chain of
// declared domain bases. UNION deduplicates domain ancestors by (relation,
// attribute, type), without treating arrays or other types as their elements.
// No deparser or name resolver consults a different catalog snapshot.
macro_rules! relation_query {
    ($relations:literal) => {
        concat!(
            "WITH RECURSIVE relations AS (",
            $relations,
            r#"), attributes AS (
    SELECT a.attrelid, a.attnum, a.attname, a.atttypid, a.atttypmod,
           a.attndims, a.attnotnull, a.atthasdef, a.attidentity,
           a.attgenerated, a.attcollation
    FROM pg_catalog.pg_attribute AS a
    JOIN relations AS r ON r.oid = a.attrelid
    WHERE a.attnum > 0 AND NOT a.attisdropped
), column_types (relation_oid, attribute_number, type_oid) AS (
    SELECT a.attrelid, a.attnum, a.atttypid FROM attributes AS a
    UNION
    SELECT ct.relation_oid, ct.attribute_number, t.typbasetype
    FROM column_types AS ct
    JOIN pg_catalog.pg_type AS t ON t.oid = ct.type_oid
    WHERE t.typtype = 'd'
)
SELECT r.oid, r.relnamespace, r.nspname::text, r.relname::text,
       r.relkind, r.relpersistence, r.relispartition,
       a.attnum, a.attname::text, a.atttypid, a.atttypmod, a.attndims,
       a.attnotnull, a.atthasdef, a.attidentity, a.attgenerated, a.attcollation,
       t.oid, t.typnamespace, tn.nspname::text, t.typname::text,
       t.typtype, t.typcategory, t.typelem, t.typbasetype, t.typtypmod,
       t.typnotnull, t.typndims
FROM relations AS r
LEFT JOIN attributes AS a ON a.attrelid = r.oid
LEFT JOIN column_types AS ct ON ct.relation_oid = r.oid
                           AND ct.attribute_number = a.attnum
LEFT JOIN pg_catalog.pg_type AS t ON t.oid = ct.type_oid
LEFT JOIN pg_catalog.pg_namespace AS tn ON tn.oid = t.typnamespace
ORDER BY r.oid, a.attnum, t.oid
"#
        )
    };
}

const RELATION_SQL: &str = relation_query!(
    r#"
    SELECT c.oid, c.relnamespace, n.nspname, c.relname, c.relkind,
           c.relpersistence, c.relispartition
    FROM pg_catalog.pg_class AS c
    JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace
    WHERE c.oid = ANY($1)
"#
);

// Native name equality allows indexed candidate lookups. Casting an input to
// `name` can truncate it, so the text comparisons separately require an exact
// match with a deterministic byte collation. DISTINCT prevents duplicate
// requests from multiplying column/domain output rows.
const NAMED_RELATION_SQL: &str = relation_query!(
    r#"
    SELECT DISTINCT c.oid, c.relnamespace, n.nspname, c.relname, c.relkind,
           c.relpersistence, c.relispartition
    FROM pg_catalog.unnest($1::text[], $2::text[])
         AS requested(schema_name, relation_name)
    JOIN pg_catalog.pg_namespace AS n
      ON n.nspname = requested.schema_name::pg_catalog.name
     AND n.nspname::text COLLATE pg_catalog."C" = requested.schema_name
    JOIN pg_catalog.pg_class AS c
      ON c.relnamespace = n.oid
     AND c.relname = requested.relation_name::pg_catalog.name
     AND c.relname::text COLLATE pg_catalog."C" = requested.relation_name
"#
);
