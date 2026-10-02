use std::fmt;

use darmok_types::{RowData, Value};
use tokio_postgres::{Column, DescribedPortal, Row, Statement};

use crate::native_value::supported_native_type;
use crate::{NativeParamUtc, NativeValueError, decode_native_row_utc};

/// A prepared statement's representation check is separate from SQL semantic
/// admission, statement execution and transaction recovery.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeStatementError {
    #[error("unsupported native PostgreSQL parameter {parameter} type (OID {oid})")]
    UnsupportedParameter { parameter: usize, oid: u32 },
    #[error("unsupported native PostgreSQL result column {column} type (OID {oid})")]
    UnsupportedColumn { column: usize, oid: u32 },
    #[error("native parameter count mismatch: expected {expected}, received {actual}")]
    ParameterCount { expected: usize, actual: usize },
    #[error("native result column count mismatch: expected {expected}, received {actual}")]
    ColumnCount { expected: usize, actual: usize },
    #[error("native result column {column} description differs in {field}")]
    ColumnDescription { column: usize, field: &'static str },
    #[error("native portal was not bound from this prepared statement handle")]
    PortalStatement,
    #[error("native portal reported NoData rather than a row description")]
    PortalNoRowDescription,
    #[error(transparent)]
    Value(#[from] NativeValueError),
}

/// Borrowed, checked native parameter/result representations. This is not an
/// executable plan or a schema-validity lease. Indices in errors are zero-based.
#[derive(Clone, Copy)]
pub struct NativeStatementUtc<'statement> {
    statement: &'statement Statement,
}

impl fmt::Debug for NativeStatementUtc<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeStatementUtc")
            .field("parameters", &self.statement.params().len())
            .field("columns", &self.statement.columns().len())
            .finish()
    }
}

impl<'statement> NativeStatementUtc<'statement> {
    /// Check the driver's prepared description before Bind/Execute. All native
    /// types must have a codec, including outputs of queries returning no rows.
    /// A supported type does not prove that every possible value is in range.
    pub fn new(statement: &'statement Statement) -> Result<Self, NativeStatementError> {
        for (parameter, ty) in statement.params().iter().enumerate() {
            if !supported_native_type(ty) {
                return Err(NativeStatementError::UnsupportedParameter {
                    parameter,
                    oid: ty.oid(),
                });
            }
        }
        for (column, metadata) in statement.columns().iter().enumerate() {
            if !supported_native_type(metadata.type_()) {
                return Err(NativeStatementError::UnsupportedColumn {
                    column,
                    oid: metadata.type_().oid(),
                });
            }
        }
        Ok(Self { statement })
    }

    /// The existing prepared description, including exact names, typmods and
    /// optional relation/attribute origins, is retained without cloning.
    pub fn statement(self) -> &'statement Statement {
        self.statement
    }

    /// Check the actual bound description before Execute. Exact handle identity
    /// and all column facts must match; this is not complete dependency validity.
    /// NoData and a zero-column RowDescription remain distinct in the receipt.
    pub fn check_portal<'portal>(
        self,
        portal: &'portal DescribedPortal,
    ) -> Result<NativePortalUtc<'statement, 'portal>, NativeStatementError> {
        if !portal.is_bound_from(self.statement) {
            return Err(NativeStatementError::PortalStatement);
        }
        check_native_columns(self.statement.columns(), portal.columns().unwrap_or(&[]))?;
        Ok(NativePortalUtc {
            description: self,
            portal,
        })
    }

    /// Values are already in dense backend binding order. This checks arity;
    /// NativeParamUtc checks individual representations during driver encoding.
    pub fn bind<'values>(
        self,
        values: &'values [Value],
    ) -> Result<NativeBindingsUtc<'statement, 'values>, NativeStatementError> {
        let expected = self.statement.params().len();
        if values.len() != expected {
            return Err(NativeStatementError::ParameterCount {
                expected,
                actual: values.len(),
            });
        }
        Ok(NativeBindingsUtc {
            description: self,
            values,
        })
    }

    /// Prevent decoding a row with a different driver description. The driver
    /// attaches cached statement metadata to rows: this comparison does not
    /// re-describe a portal or detect external DDL.
    pub fn decode_row(self, row: &Row) -> Result<RowData, NativeStatementError> {
        decode_native_columns_utc(self.statement.columns(), row)
    }
}

/// A checked prepared handle and its matching native bound description.
/// It cannot execute SQL, finish a scope or establish all catalog dependencies.
#[derive(Clone, Copy)]
pub struct NativePortalUtc<'statement, 'portal> {
    description: NativeStatementUtc<'statement>,
    portal: &'portal DescribedPortal,
}

impl fmt::Debug for NativePortalUtc<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativePortalUtc")
            .field("description", &self.description)
            .field("portal", &self.portal)
            .finish()
    }
}

impl<'statement, 'portal> NativePortalUtc<'statement, 'portal> {
    pub fn portal(self) -> &'portal DescribedPortal {
        self.portal
    }
    pub fn statement_description(self) -> NativeStatementUtc<'statement> {
        self.description
    }
    pub fn columns(self) -> Option<&'portal [Column]> {
        self.portal.columns()
    }

    /// Rows carry the portal's observed description, rather than a cached
    /// statement description. Range checks still occur for each actual value.
    pub fn decode_row(self, row: &Row) -> Result<RowData, NativeStatementError> {
        let columns = self
            .columns()
            .ok_or(NativeStatementError::PortalNoRowDescription)?;
        decode_native_columns_utc(columns, row)
    }
}

pub(crate) fn decode_native_columns_utc(
    expected: &[Column],
    row: &Row,
) -> Result<RowData, NativeStatementError> {
    check_native_columns(expected, row.columns())?;
    Ok(decode_native_row_utc(row)?)
}

fn check_native_columns(
    expected: &[Column],
    actual: &[Column],
) -> Result<(), NativeStatementError> {
    if actual.len() != expected.len() {
        return Err(NativeStatementError::ColumnCount {
            expected: expected.len(),
            actual: actual.len(),
        });
    }
    // A statement or described portal shares immutable metadata with its
    // rows. Identity skips repeated comparison after the bound check.
    if !std::ptr::eq(expected, actual) {
        for (column, (expected, actual)) in expected.iter().zip(actual).enumerate() {
            let field = if expected.name() != actual.name() {
                Some("name")
            } else if expected.type_() != actual.type_() {
                Some("type")
            } else if expected.type_modifier() != actual.type_modifier() {
                Some("type modifier")
            } else if expected.table_oid() != actual.table_oid() {
                Some("relation origin")
            } else if expected.column_id() != actual.column_id() {
                Some("attribute origin")
            } else {
                None
            };
            if let Some(field) = field {
                return Err(NativeStatementError::ColumnDescription { column, field });
            }
        }
    }
    Ok(())
}

/// Borrowed parameters tied to their checked statement and backend arity.
#[derive(Clone, Copy)]
pub struct NativeBindingsUtc<'statement, 'values> {
    description: NativeStatementUtc<'statement>,
    values: &'values [Value],
}

impl fmt::Debug for NativeBindingsUtc<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeBindingsUtc")
            .field("description", &self.description)
            .finish()
    }
}

impl<'statement, 'values> NativeBindingsUtc<'statement, 'values> {
    pub fn statement(self) -> &'statement Statement {
        self.description.statement()
    }

    /// No value copies, intermediate vector or second encoding pass are needed.
    pub fn parameters(self) -> impl ExactSizeIterator<Item = NativeParamUtc<'values>> {
        self.values.iter().map(NativeParamUtc::new)
    }
}
