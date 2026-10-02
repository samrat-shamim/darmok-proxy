//! Fresh native PostgreSQL relation and declared-type facts by database-local
//! OID or exact native schema/relation names in one statement snapshot.
//!
//! These snapshots do not establish query nullability, MySQL attributes,
//! statement validity or a reusable catalog generation. See the catalog
//! contract in `docs/native-catalog.md` before using them in execution.

mod observation;
mod relation;

pub use observation::{
    CATALOG_REQUEST_MAX_BYTES, CATALOG_REQUEST_MAX_PAIRS, CATALOG_RESPONSE_MAX_BYTES,
    CatalogObservation, CatalogObservationError, NativeCatalogStamp, decode_catalog_observation,
};

pub use relation::{
    CatalogError, ColumnGeneration, ColumnIdentity, DomainType, NamedNativeCatalog, NativeCatalog,
    NativeColumn, NativeRelation, NativeRelationName, NativeType, RelationKind,
    RelationPersistence, TypeKind, read_native_named_relations, read_native_relations,
};
