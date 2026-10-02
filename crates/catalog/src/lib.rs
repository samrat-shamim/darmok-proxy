//! Fresh native PostgreSQL relation and declared-type facts by database-local
//! OID or exact native schema/relation names in one statement snapshot.
//!
//! These snapshots do not establish query nullability, MySQL attributes,
//! statement validity or a reusable catalog generation. See the catalog
//! contract in `docs/native-catalog.md` before using them in execution.

mod relation;

pub use relation::{
    CatalogError, ColumnGeneration, ColumnIdentity, DomainType, NamedNativeCatalog, NativeCatalog,
    NativeColumn, NativeRelation, NativeRelationName, NativeType, RelationKind,
    RelationPersistence, TypeKind, read_native_named_relations, read_native_relations,
};
