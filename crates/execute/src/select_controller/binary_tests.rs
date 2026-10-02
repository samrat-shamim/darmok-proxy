//! Captured stock field declarations and values; no computed expected digits.
use super::*;
use darmok_session::{AutocommitSetting, MysqlCompatibilityProfile, SqlModes};
use sqlparser::source::parse_mysql_source;

fn fixture(mode: &str) -> (SessionState, ServerSetValues) {
    let mut state = SessionState::new(1);
    state
        .set_sql_modes(SqlModes::parse_names(mode).unwrap())
        .unwrap();
    let globals = ServerSetValues {
        sql_modes: SqlModes::MYSQL84_DEFAULT,
        transactions: MysqlCompatibilityProfile::default_mysql8()
            .default_transaction_characteristics,
        autocommit: AutocommitSetting::Enabled,
        completion_type: darmok_session::FrontendCompletionType::NoChain,
    };
    (state, globals)
}

pub(crate) fn corpus() -> serde_json::Value {
    serde_json::from_str(include_str!("../../fixtures/mysql-binary-literals.json")).unwrap()
}

pub(crate) fn unhex(source: &str) -> Vec<u8> {
    assert_eq!(source.len() % 2, 0);
    (0..source.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&source[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn binary_literals_match_all_captured_stock_metadata_and_bytes() {
    let data = corpus();
    assert_eq!(data["cases"].as_array().unwrap().len(), 44);
    for case in data["cases"].as_array().unwrap() {
        let (state, globals) = fixture(case["sql_mode"].as_str().unwrap());
        let parsed = parse_mysql_source(
            case["sql"].as_str().unwrap(),
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        let expected_columns = case["columns"].as_array().unwrap();
        assert_eq!(plan.columns.len(), expected_columns.len(), "{case}");
        for (column, expected) in plan.columns.iter().zip(expected_columns) {
            assert_eq!(
                column.name.as_ref(),
                unhex(expected["name_hex"].as_str().unwrap()),
                "{case}"
            );
            assert_eq!(
                u64::from(column.column_type),
                expected["type"].as_u64().unwrap()
            );
            assert_eq!(u64::from(column.flags), expected["flags"].as_u64().unwrap());
            assert_eq!(
                u64::from(column.character_set),
                expected["charset"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(column.column_length),
                expected["width"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(column.decimals),
                expected["decimals"].as_u64().unwrap()
            );
            assert_eq!(
                column.schema.as_ref(),
                unhex(expected["schema_hex"].as_str().unwrap())
            );
            assert_eq!(
                column.table.as_ref(),
                unhex(expected["table_hex"].as_str().unwrap())
            );
            assert_eq!(
                column.org_table.as_ref(),
                unhex(expected["org_table_hex"].as_str().unwrap())
            );
            assert_eq!(
                column.org_name.as_ref(),
                unhex(expected["org_name_hex"].as_str().unwrap())
            );
        }
        let expected_row = case["rows_hex"][0].as_array().unwrap();
        assert_eq!(plan.row.len(), expected_row.len());
        for (cell, expected) in plan.row.iter().zip(expected_row) {
            match expected.as_str() {
                Some(hex) => assert_eq!(cell.as_ref().unwrap().as_ref(), unhex(hex), "{case}"),
                None => assert!(cell.is_none()),
            }
        }
    }
}

#[test]
fn quoted_binary_syntax_and_unimplemented_contexts_never_become_results() {
    let data = corpus();
    for case in data["syntax_errors"].as_array().unwrap() {
        let (state, _) = fixture(case["sql_mode"].as_str().unwrap());
        assert!(
            parse_mysql_source(
                case["sql"].as_str().unwrap(),
                state.sql_modes().unwrap().parser_flags()
            )
            .is_err(),
            "{case}"
        );
    }
    for case in data["unsupported"].as_array().unwrap() {
        let (state, globals) = fixture(case["sql_mode"].as_str().unwrap());
        let parsed = parse_mysql_source(
            case["sql"].as_str().unwrap(),
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap();
        assert!(
            matches!(
                admit_select(&state, &globals, parsed.single_select().unwrap().unwrap()),
                Err(SelectAdmissionError::Sql(SelectSqlError::Unsupported))
            ),
            "{case}"
        );
    }
}
