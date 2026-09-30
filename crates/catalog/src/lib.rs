//! Fresh, native PostgreSQL relation and declared-type facts.
//!
//! These snapshots do not establish query nullability, MySQL attributes,
//! statement validity or a reusable catalog generation. See the catalog
//! contract in `docs/native-catalog.md` before using them in execution.

mod relation;

pub use relation::{
    CatalogError, ColumnGeneration, ColumnIdentity, DomainType, NativeCatalog, NativeColumn,
    NativeRelation, NativeType, RelationKind, RelationPersistence, TypeKind, read_native_relations,
};
