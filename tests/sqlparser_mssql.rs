// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

#![warn(clippy::all)]
//! Test SQL syntax specific to Microsoft's T-SQL. The parser based on the
//! generic dialect is also tested (on the inputs it can handle).

#[macro_use]
mod test_utils;

use helpers::attached_token::AttachedToken;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Word};
use test_utils::*;

use sqlparser::ast::DataType::{Int, Text, Varbinary};
use sqlparser::ast::DeclareAssignment::MsSqlAssignment;
use sqlparser::ast::Value::SingleQuotedString;
use sqlparser::ast::*;
use sqlparser::dialect::{GenericDialect, MsSqlDialect};
use sqlparser::parser::{Parser, ParserError, ParserOptions};

#[test]
fn parse_mssql_identifiers() {
    let sql = "SELECT @@version, _foo$123 FROM ##temp";
    let select = ms_and_generic().verified_only_select(sql);
    assert_eq!(
        &Expr::Identifier(Ident::new("@@version")),
        expr_from_projection(&select.projection[0]),
    );
    assert_eq!(
        &Expr::Identifier(Ident::new("_foo$123")),
        expr_from_projection(&select.projection[1]),
    );
    assert_eq!(2, select.projection.len());
    match &only(&select.from).relation {
        TableFactor::Table { name, .. } => {
            assert_eq!("##temp".to_string(), name.to_string());
        }
        _ => unreachable!(),
    };
}

#[test]
fn parse_table_time_travel() {
    let version = "2023-08-18 23:08:18".to_string();
    let sql = format!("SELECT 1 FROM t1 FOR SYSTEM_TIME AS OF '{version}'");
    let select = ms().verified_only_select(&sql);
    assert_eq!(
        select.from,
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t1")]),
                alias: None,
                args: None,
                with_hints: vec![],
                version: Some(TableVersion::ForSystemTimeAsOf(Expr::Value(
                    (Value::SingleQuotedString(version)).with_empty_span()
                ))),
                partitions: vec![],
                with_ordinality: false,
                json_path: None,
                sample: None,
                index_hints: vec![]
            },
            joins: vec![]
        },]
    );

    let sql = "SELECT 1 FROM t1 FOR SYSTEM TIME AS OF 'some_timestamp'".to_string();
    assert!(ms().parse_sql_statements(&sql).is_err());
}

#[test]
fn parse_mssql_single_quoted_aliases() {
    let _ = ms_and_generic().one_statement_parses_to("SELECT foo 'alias'", "SELECT foo AS 'alias'");
}

#[test]
fn parse_mssql_delimited_identifiers() {
    let _ = ms().one_statement_parses_to(
        "SELECT [a.b!] [FROM] FROM foo [WHERE]",
        "SELECT [a.b!] AS [FROM] FROM foo AS [WHERE]",
    );
}

#[test]
fn parse_create_procedure() {
    let sql = "CREATE OR ALTER PROCEDURE test (@foo INT, @bar VARCHAR(256)) AS BEGIN SELECT 1; END";

    assert_eq!(
        ms().verified_stmt(sql),
        Statement::CreateProcedure {
            or_alter: true,
            body: ConditionalStatements::BeginEnd(BeginEndStatements {
                begin_token: AttachedToken::empty(),
                statements: vec![Statement::Query(Box::new(Query {
                    with: None,
                    limit_clause: None,
                    fetch: None,
                    locks: vec![],
                    for_clause: None,
                    order_by: None,
                    settings: None,
                    format_clause: None,
                    pipe_operators: vec![],
                    body: Box::new(SetExpr::Select(Box::new(Select {
                        select_token: AttachedToken::empty(),
                        distinct: None,
                        top: None,
                        top_before_distinct: false,
                        projection: vec![SelectItem::UnnamedExpr(Expr::Value(
                            (number("1")).with_empty_span()
                        ))],
                        exclude: None,
                        into: None,
                        from: vec![],
                        lateral_views: vec![],
                        prewhere: None,
                        selection: None,
                        group_by: GroupByExpr::Expressions(vec![], vec![]),
                        cluster_by: vec![],
                        distribute_by: vec![],
                        sort_by: vec![],
                        having: None,
                        named_window: vec![],
                        window_before_qualify: false,
                        qualify: None,
                        value_table_mode: None,
                        connect_by: None,
                        flavor: SelectFlavor::Standard,
                    })))
                }))],
                end_token: AttachedToken::empty(),
            }),
            params: Some(vec![
                ProcedureParam {
                    name: Ident {
                        value: "@foo".into(),
                        quote_style: None,
                        span: Span::empty(),
                    },
                    data_type: DataType::Int(None),
                    mode: None,
                },
                ProcedureParam {
                    name: Ident {
                        value: "@bar".into(),
                        quote_style: None,
                        span: Span::empty(),
                    },
                    data_type: DataType::Varchar(Some(CharacterLength::IntegerLength {
                        length: 256,
                        unit: None
                    })),
                    mode: None,
                }
            ]),
            name: ObjectName::from(vec![Ident {
                value: "test".into(),
                quote_style: None,
                span: Span::empty(),
            }]),
            language: None,
        }
    )
}

#[test]
fn parse_mssql_create_procedure() {
    let _ = ms_and_generic().verified_stmt("CREATE OR ALTER PROCEDURE foo AS SELECT 1;");
    let _ = ms_and_generic().verified_stmt("CREATE OR ALTER PROCEDURE foo AS BEGIN SELECT 1; END");
    let _ = ms_and_generic().verified_stmt("CREATE PROCEDURE foo AS BEGIN SELECT 1; END");
    let _ = ms().verified_stmt(
        "CREATE PROCEDURE foo AS BEGIN SELECT [myColumn] FROM [myschema].[mytable]; END",
    );
    let _ = ms_and_generic().verified_stmt(
        "CREATE PROCEDURE foo (@CustomerName NVARCHAR(50)) AS BEGIN SELECT * FROM DEV; END",
    );
    let _ = ms().verified_stmt("CREATE PROCEDURE [foo] AS BEGIN UPDATE bar SET col = 'test'; END");
    // Test a statement with END in it
    let _ = ms().verified_stmt("CREATE PROCEDURE [foo] AS BEGIN SELECT [foo], CASE WHEN [foo] IS NULL THEN 'empty' ELSE 'notempty' END AS [foo]; END");
    // Multiple statements
    let _ = ms().verified_stmt("CREATE PROCEDURE [foo] AS BEGIN UPDATE bar SET col = 'test'; SELECT [foo] FROM BAR WHERE [FOO] > 10; END");
}

#[test]
fn parse_create_function() {
    let return_expression_function = "CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT AS BEGIN RETURN 1; END";
    assert_eq!(
        ms().verified_stmt(return_expression_function),
        sqlparser::ast::Statement::CreateFunction(CreateFunction {
            or_alter: false,
            or_replace: false,
            temporary: false,
            if_not_exists: false,
            name: ObjectName::from(vec![Ident::new("some_scalar_udf")]),
            args: Some(vec![
                OperateFunctionArg {
                    mode: None,
                    name: Some(Ident::new("@foo")),
                    data_type: DataType::Int(None),
                    default_expr: None,
                },
                OperateFunctionArg {
                    mode: None,
                    name: Some(Ident::new("@bar")),
                    data_type: DataType::Varchar(Some(CharacterLength::IntegerLength {
                        length: 256,
                        unit: None
                    })),
                    default_expr: None,
                },
            ]),
            return_type: Some(DataType::Int(None)),
            function_body: Some(CreateFunctionBody::AsBeginEnd(BeginEndStatements {
                begin_token: AttachedToken::empty(),
                statements: vec![Statement::Return(ReturnStatement {
                    value: Some(ReturnStatementValue::Expr(Expr::Value(
                        (number("1")).with_empty_span()
                    ))),
                })],
                end_token: AttachedToken::empty(),
            })),
            behavior: None,
            called_on_null: None,
            parallel: None,
            using: None,
            language: None,
            determinism_specifier: None,
            options: None,
            remote_connection: None,
        }),
    );

    let multi_statement_function = "\
        CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) \
        RETURNS INT \
        AS \
        BEGIN \
            SET @foo = @foo + 1; \
            RETURN @foo; \
        END\
    ";
    let _ = ms().verified_stmt(multi_statement_function);

    let multi_statement_function_without_as = multi_statement_function.replace(" AS", "");
    let _ = ms().one_statement_parses_to(
        &multi_statement_function_without_as,
        multi_statement_function,
    );

    let create_function_with_conditional = "\
        CREATE FUNCTION some_scalar_udf() \
        RETURNS INT \
        AS \
        BEGIN \
            IF 1 = 2 \
            BEGIN \
                RETURN 1; \
            END; \
            RETURN 0; \
        END\
    ";
    let _ = ms().verified_stmt(create_function_with_conditional);

    let create_or_alter_function = "\
        CREATE OR ALTER FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) \
        RETURNS INT \
        AS \
        BEGIN \
            SET @foo = @foo + 1; \
            RETURN @foo; \
        END\
    ";
    let _ = ms().verified_stmt(create_or_alter_function);

    let create_function_with_return_expression = "\
        CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) \
        RETURNS INT \
        AS \
        BEGIN \
            RETURN CONVERT(INT, 1) + 2; \
        END\
    ";
    let _ = ms().verified_stmt(create_function_with_return_expression);

    let create_inline_table_value_function = "\
        CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS TABLE \
        AS \
        RETURN (SELECT 1 AS col_1)\
    ";
    let _ = ms().verified_stmt(create_inline_table_value_function);

    let create_inline_table_value_function_without_parentheses = "\
        CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS TABLE \
        AS \
        RETURN SELECT 1 AS col_1\
    ";
    let _ = ms().verified_stmt(create_inline_table_value_function_without_parentheses);

    let create_inline_table_value_function_without_as =
        create_inline_table_value_function.replace(" AS", "");
    let _ = ms().one_statement_parses_to(
        &create_inline_table_value_function_without_as,
        create_inline_table_value_function,
    );

    let create_multi_statement_table_value_function = "\
        CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS @t TABLE (col_1 INT) \
        AS \
        BEGIN \
            INSERT INTO @t SELECT 1; \
            RETURN; \
        END\
    ";
    let _ = ms().verified_stmt(create_multi_statement_table_value_function);

    let create_multi_statement_table_value_function_without_as =
        create_multi_statement_table_value_function.replace(" AS", "");
    let _ = ms().one_statement_parses_to(
        &create_multi_statement_table_value_function_without_as,
        create_multi_statement_table_value_function,
    );

    let create_multi_statement_table_value_function_with_constraints = "\
        CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS @t TABLE (col_1 INT NOT NULL) \
        AS \
        BEGIN \
            INSERT INTO @t SELECT 1; \
            RETURN @t; \
        END\
    ";
    let _ = ms().verified_stmt(create_multi_statement_table_value_function_with_constraints);

    let create_multi_statement_tvf_without_table_definition = "\
        CREATE FUNCTION incorrect_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS @t TABLE ()
        AS \
        BEGIN \
            INSERT INTO @t SELECT 1; \
            RETURN @t; \
        END\
    ";
    assert_eq!(
        ParserError::ParserError("Unparsable function body".to_owned()),
        ms().parse_sql_statements(create_multi_statement_tvf_without_table_definition)
            .unwrap_err()
    );

    let create_inline_tvf_without_subquery_or_bare_select = "\
        CREATE FUNCTION incorrect_tvf(@foo INT, @bar VARCHAR(256)) \
        RETURNS TABLE
        AS \
        RETURN 'hi'\
    ";
    assert_eq!(
        ParserError::ParserError(
            "Expected a subquery (or bare SELECT statement) after RETURN".to_owned()
        ),
        ms().parse_sql_statements(create_inline_tvf_without_subquery_or_bare_select)
            .unwrap_err()
    );
}

#[test]
fn parse_create_function_parameter_default_values() {
    let single_default_sql =
        "CREATE FUNCTION test_func(@param1 INT = 42) RETURNS INT AS BEGIN RETURN @param1; END";
    assert_eq!(
        ms().verified_stmt(single_default_sql),
        Statement::CreateFunction(CreateFunction {
            or_alter: false,
            or_replace: false,
            temporary: false,
            if_not_exists: false,
            name: ObjectName::from(vec![Ident::new("test_func")]),
            args: Some(vec![OperateFunctionArg {
                mode: None,
                name: Some(Ident::new("@param1")),
                data_type: DataType::Int(None),
                default_expr: Some(Expr::Value((number("42")).with_empty_span())),
            },]),
            return_type: Some(DataType::Int(None)),
            function_body: Some(CreateFunctionBody::AsBeginEnd(BeginEndStatements {
                begin_token: AttachedToken::empty(),
                statements: vec![Statement::Return(ReturnStatement {
                    value: Some(ReturnStatementValue::Expr(Expr::Identifier(Ident::new(
                        "@param1"
                    )))),
                })],
                end_token: AttachedToken::empty(),
            })),
            behavior: None,
            called_on_null: None,
            parallel: None,
            using: None,
            language: None,
            determinism_specifier: None,
            options: None,
            remote_connection: None,
        }),
    );
}

#[test]
fn parse_mssql_apply_join() {
    let _ = ms_and_generic().verified_only_select(
        "SELECT * FROM sys.dm_exec_query_stats AS deqs \
         CROSS APPLY sys.dm_exec_query_plan(deqs.plan_handle)",
    );
    let _ = ms_and_generic().verified_only_select(
        "SELECT * FROM sys.dm_exec_query_stats AS deqs \
         OUTER APPLY sys.dm_exec_query_plan(deqs.plan_handle)",
    );
    let _ = ms_and_generic().verified_only_select(
        "SELECT * FROM foo \
         OUTER APPLY (SELECT foo.x + 1) AS bar",
    );
}

#[test]
fn parse_mssql_openjson() {
    let select = ms().verified_only_select(
        "SELECT B.kind, B.id_list \
        FROM t_test_table AS A \
        CROSS APPLY OPENJSON(A.param, '$.config') WITH (kind VARCHAR(20) '$.kind', [id_list] NVARCHAR(MAX) '$.id_list' AS JSON) AS B",
    );
    assert_eq!(
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t_test_table")]),
                alias: Some(TableAlias {
                    name: Ident::new("A"),
                    columns: vec![]
                }),
                args: None,
                with_hints: vec![],
                version: None,
                with_ordinality: false,
                partitions: vec![],
                json_path: None,
                sample: None,
                index_hints: vec![]
            },
            joins: vec![Join {
                relation: TableFactor::OpenJsonTable {
                    json_expr: Expr::CompoundIdentifier(
                        vec![Ident::new("A"), Ident::new("param"),]
                    ),
                    json_path: Some(Value::SingleQuotedString("$.config".into())),
                    columns: vec![
                        OpenJsonTableColumn {
                            name: Ident::new("kind"),
                            r#type: DataType::Varchar(Some(CharacterLength::IntegerLength {
                                length: 20,
                                unit: None
                            })),
                            path: Some("$.kind".into()),
                            as_json: false
                        },
                        OpenJsonTableColumn {
                            name: Ident {
                                value: "id_list".into(),
                                quote_style: Some('['),
                                span: Span::empty(),
                            },
                            r#type: DataType::Nvarchar(Some(CharacterLength::Max)),
                            path: Some("$.id_list".into()),
                            as_json: true
                        }
                    ],
                    alias: Some(TableAlias {
                        name: Ident::new("B"),
                        columns: vec![]
                    })
                },
                global: false,
                join_operator: JoinOperator::CrossApply
            }]
        }],
        select.from
    );
    let select = ms().verified_only_select(
        "SELECT B.kind, B.id_list \
        FROM t_test_table AS A \
        CROSS APPLY OPENJSON(A.param) WITH (kind VARCHAR(20) '$.kind', [id_list] NVARCHAR(MAX) '$.id_list' AS JSON) AS B",
    );
    assert_eq!(
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t_test_table"),]),
                alias: Some(TableAlias {
                    name: Ident::new("A"),
                    columns: vec![]
                }),
                args: None,
                with_hints: vec![],
                version: None,
                with_ordinality: false,
                partitions: vec![],
                json_path: None,
                sample: None,
                index_hints: vec![]
            },
            joins: vec![Join {
                relation: TableFactor::OpenJsonTable {
                    json_expr: Expr::CompoundIdentifier(
                        vec![Ident::new("A"), Ident::new("param"),]
                    ),
                    json_path: None,
                    columns: vec![
                        OpenJsonTableColumn {
                            name: Ident::new("kind"),
                            r#type: DataType::Varchar(Some(CharacterLength::IntegerLength {
                                length: 20,
                                unit: None
                            })),
                            path: Some("$.kind".into()),
                            as_json: false
                        },
                        OpenJsonTableColumn {
                            name: Ident {
                                value: "id_list".into(),
                                quote_style: Some('['),
                                span: Span::empty(),
                            },
                            r#type: DataType::Nvarchar(Some(CharacterLength::Max)),
                            path: Some("$.id_list".into()),
                            as_json: true
                        }
                    ],
                    alias: Some(TableAlias {
                        name: Ident::new("B"),
                        columns: vec![]
                    })
                },
                global: false,
                join_operator: JoinOperator::CrossApply
            }]
        }],
        select.from
    );
    let select = ms().verified_only_select(
        "SELECT B.kind, B.id_list \
        FROM t_test_table AS A \
        CROSS APPLY OPENJSON(A.param) WITH (kind VARCHAR(20), [id_list] NVARCHAR(MAX)) AS B",
    );
    assert_eq!(
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t_test_table")]),
                alias: Some(TableAlias {
                    name: Ident::new("A"),
                    columns: vec![]
                }),
                args: None,
                with_hints: vec![],
                version: None,
                with_ordinality: false,
                partitions: vec![],
                json_path: None,
                sample: None,
                index_hints: vec![]
            },
            joins: vec![Join {
                relation: TableFactor::OpenJsonTable {
                    json_expr: Expr::CompoundIdentifier(
                        vec![Ident::new("A"), Ident::new("param"),]
                    ),
                    json_path: None,
                    columns: vec![
                        OpenJsonTableColumn {
                            name: Ident::new("kind"),
                            r#type: DataType::Varchar(Some(CharacterLength::IntegerLength {
                                length: 20,
                                unit: None
                            })),
                            path: None,
                            as_json: false
                        },
                        OpenJsonTableColumn {
                            name: Ident {
                                value: "id_list".into(),
                                quote_style: Some('['),
                                span: Span::empty(),
                            },
                            r#type: DataType::Nvarchar(Some(CharacterLength::Max)),
                            path: None,
                            as_json: false
                        }
                    ],
                    alias: Some(TableAlias {
                        name: Ident::new("B"),
                        columns: vec![]
                    })
                },
                global: false,
                join_operator: JoinOperator::CrossApply
            }]
        }],
        select.from
    );
    let select = ms_and_generic().verified_only_select(
        "SELECT B.kind, B.id_list \
        FROM t_test_table AS A \
        CROSS APPLY OPENJSON(A.param, '$.config') AS B",
    );
    assert_eq!(
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t_test_table")]),
                alias: Some(TableAlias {
                    name: Ident::new("A"),
                    columns: vec![]
                }),
                args: None,
                with_hints: vec![],
                version: None,
                with_ordinality: false,
                partitions: vec![],
                json_path: None,
                sample: None,
                index_hints: vec![],
            },
            joins: vec![Join {
                relation: TableFactor::OpenJsonTable {
                    json_expr: Expr::CompoundIdentifier(
                        vec![Ident::new("A"), Ident::new("param"),]
                    ),
                    json_path: Some(Value::SingleQuotedString("$.config".into())),
                    columns: vec![],
                    alias: Some(TableAlias {
                        name: Ident::new("B"),
                        columns: vec![]
                    })
                },
                global: false,
                join_operator: JoinOperator::CrossApply
            }]
        }],
        select.from
    );
    let select = ms_and_generic().verified_only_select(
        "SELECT B.kind, B.id_list \
        FROM t_test_table AS A \
        CROSS APPLY OPENJSON(A.param) AS B",
    );
    assert_eq!(
        vec![TableWithJoins {
            relation: TableFactor::Table {
                name: ObjectName::from(vec![Ident::new("t_test_table")]),
                alias: Some(TableAlias {
                    name: Ident::new("A"),
                    columns: vec![]
                }),
                args: None,
                with_hints: vec![],
                version: None,
                with_ordinality: false,
                partitions: vec![],
                json_path: None,
                sample: None,
                index_hints: vec![],
            },
            joins: vec![Join {
                relation: TableFactor::OpenJsonTable {
                    json_expr: Expr::CompoundIdentifier(
                        vec![Ident::new("A"), Ident::new("param"),]
                    ),
                    json_path: None,
                    columns: vec![],
                    alias: Some(TableAlias {
                        name: Ident::new("B"),
                        columns: vec![]
                    })
                },
                global: false,
                join_operator: JoinOperator::CrossApply
            }]
        }],
        select.from
    );
}

#[test]
fn parse_mssql_top_paren() {
    let sql = "SELECT TOP (5) * FROM foo";
    let select = ms_and_generic().verified_only_select(sql);
    let top = select.top.unwrap();
    assert_eq!(
        Some(TopQuantity::Expr(Expr::Value(
            (number("5")).with_empty_span()
        ))),
        top.quantity
    );
    assert!(!top.percent);
}

#[test]
fn parse_mssql_top_percent() {
    let sql = "SELECT TOP (5) PERCENT * FROM foo";
    let select = ms_and_generic().verified_only_select(sql);
    let top = select.top.unwrap();
    assert_eq!(
        Some(TopQuantity::Expr(Expr::Value(
            (number("5")).with_empty_span()
        ))),
        top.quantity
    );
    assert!(top.percent);
}

#[test]
fn parse_mssql_top_with_ties() {
    let sql = "SELECT TOP (5) WITH TIES * FROM foo";
    let select = ms_and_generic().verified_only_select(sql);
    let top = select.top.unwrap();
    assert_eq!(
        Some(TopQuantity::Expr(Expr::Value(
            (number("5")).with_empty_span()
        ))),
        top.quantity
    );
    assert!(top.with_ties);
}

#[test]
fn parse_mssql_top_percent_with_ties() {
    let sql = "SELECT TOP (10) PERCENT WITH TIES * FROM foo";
    let select = ms_and_generic().verified_only_select(sql);
    let top = select.top.unwrap();
    assert_eq!(
        Some(TopQuantity::Expr(Expr::Value(
            (number("10")).with_empty_span()
        ))),
        top.quantity
    );
    assert!(top.percent);
}

#[test]
fn parse_mssql_top() {
    let sql = "SELECT TOP 5 bar, baz FROM foo";
    let _ = ms_and_generic().one_statement_parses_to(sql, "SELECT TOP 5 bar, baz FROM foo");
}

#[test]
fn parse_mssql_bin_literal() {
    let _ = ms_and_generic().one_statement_parses_to("SELECT 0xdeadBEEF", "SELECT X'deadBEEF'");
}

#[test]
fn parse_mssql_create_role() {
    let sql = "CREATE ROLE mssql AUTHORIZATION helena";
    match ms().verified_stmt(sql) {
        Statement::CreateRole {
            names,
            authorization_owner,
            ..
        } => {
            assert_eq_vec(&["mssql"], &names);
            assert_eq!(
                authorization_owner,
                Some(ObjectName::from(vec![Ident {
                    value: "helena".into(),
                    quote_style: None,
                    span: Span::empty(),
                }]))
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_alter_role() {
    let sql = "ALTER ROLE old_name WITH NAME = new_name";
    assert_eq!(
        ms().parse_sql_statements(sql).unwrap(),
        [Statement::AlterRole {
            name: Ident {
                value: "old_name".into(),
                quote_style: None,
                span: Span::empty(),
            },
            operation: AlterRoleOperation::RenameRole {
                role_name: Ident {
                    value: "new_name".into(),
                    quote_style: None,
                    span: Span::empty(),
                }
            },
        }]
    );

    let sql = "ALTER ROLE role_name ADD MEMBER new_member";
    assert_eq!(
        ms().verified_stmt(sql),
        Statement::AlterRole {
            name: Ident {
                value: "role_name".into(),
                quote_style: None,
                span: Span::empty(),
            },
            operation: AlterRoleOperation::AddMember {
                member_name: Ident {
                    value: "new_member".into(),
                    quote_style: None,
                    span: Span::empty(),
                }
            },
        }
    );

    let sql = "ALTER ROLE role_name DROP MEMBER old_member";
    assert_eq!(
        ms().verified_stmt(sql),
        Statement::AlterRole {
            name: Ident {
                value: "role_name".into(),
                quote_style: None,
                span: Span::empty(),
            },
            operation: AlterRoleOperation::DropMember {
                member_name: Ident {
                    value: "old_member".into(),
                    quote_style: None,
                    span: Span::empty(),
                }
            },
        }
    );
}

#[test]
fn parse_delimited_identifiers() {
    // check that quoted identifiers in any position remain quoted after serialization
    let select = ms_and_generic().verified_only_select(
        r#"SELECT "alias"."bar baz", "myfun"(), "simple id" AS "column alias" FROM "a table" AS "alias""#,
    );
    // check FROM
    match only(select.from).relation {
        TableFactor::Table {
            name,
            alias,
            args,
            with_hints,
            version,
            ..
        } => {
            assert_eq!(
                ObjectName::from(vec![Ident::with_quote('"', "a table")]),
                name
            );
            assert_eq!(Ident::with_quote('"', "alias"), alias.unwrap().name);
            assert!(args.is_none());
            assert!(with_hints.is_empty());
            assert!(version.is_none());
        }
        _ => panic!("Expecting TableFactor::Table"),
    }
    // check SELECT
    assert_eq!(3, select.projection.len());
    assert_eq!(
        &Expr::CompoundIdentifier(vec![
            Ident::with_quote('"', "alias"),
            Ident::with_quote('"', "bar baz"),
        ]),
        expr_from_projection(&select.projection[0]),
    );
    assert_eq!(
        &Expr::Function(Function {
            name: ObjectName::from(vec![Ident::with_quote('"', "myfun")]),
            uses_odbc_syntax: false,
            parameters: FunctionArguments::None,
            args: FunctionArguments::List(FunctionArgumentList {
                duplicate_treatment: None,
                args: vec![],
                clauses: vec![],
            }),
            null_treatment: None,
            filter: None,
            over: None,
            within_group: vec![],
        }),
        expr_from_projection(&select.projection[1]),
    );
    match &select.projection[2] {
        SelectItem::ExprWithAlias { expr, alias } => {
            assert_eq!(&Expr::Identifier(Ident::with_quote('"', "simple id")), expr);
            assert_eq!(&Ident::with_quote('"', "column alias"), alias);
        }
        _ => panic!("Expected ExprWithAlias"),
    }

    ms_and_generic().verified_stmt(r#"CREATE TABLE "foo" ("bar" "int")"#);
    ms_and_generic().verified_stmt(r#"ALTER TABLE foo ADD CONSTRAINT "bar" PRIMARY KEY (baz)"#);
    //TODO verified_stmt(r#"UPDATE foo SET "bar" = 5"#);
}

#[test]
fn parse_table_name_in_square_brackets() {
    let select = ms().verified_only_select(r#"SELECT [a column] FROM [a schema].[a table]"#);
    if let TableFactor::Table { name, .. } = only(select.from).relation {
        assert_eq!(
            ObjectName::from(vec![
                Ident::with_quote('[', "a schema"),
                Ident::with_quote('[', "a table")
            ]),
            name
        );
    } else {
        panic!("Expecting TableFactor::Table");
    }
    assert_eq!(
        &Expr::Identifier(Ident::with_quote('[', "a column")),
        expr_from_projection(&select.projection[0]),
    );
}

#[test]
fn parse_for_clause() {
    ms_and_generic().verified_stmt("SELECT a FROM t FOR JSON PATH");
    ms_and_generic().verified_stmt("SELECT b FROM t FOR JSON AUTO");
    ms_and_generic().verified_stmt("SELECT c FROM t FOR JSON AUTO, WITHOUT_ARRAY_WRAPPER");
    ms_and_generic().verified_stmt("SELECT 1 FROM t FOR JSON PATH, ROOT('x'), INCLUDE_NULL_VALUES");
    ms_and_generic().verified_stmt("SELECT 2 FROM t FOR XML AUTO");
    ms_and_generic().verified_stmt("SELECT 3 FROM t FOR XML AUTO, TYPE, ELEMENTS");
    ms_and_generic().verified_stmt("SELECT * FROM t WHERE x FOR XML AUTO, ELEMENTS");
    ms_and_generic().verified_stmt("SELECT x FROM t ORDER BY y FOR XML AUTO, ELEMENTS");
    ms_and_generic().verified_stmt("SELECT y FROM t FOR XML PATH('x'), ROOT('y'), ELEMENTS");
    ms_and_generic().verified_stmt("SELECT z FROM t FOR XML EXPLICIT, BINARY BASE64");
    ms_and_generic().verified_stmt("SELECT * FROM t FOR XML RAW('x')");
    ms_and_generic().verified_stmt("SELECT * FROM t FOR BROWSE");
}

#[test]
fn dont_parse_trailing_for() {
    assert!(ms()
        .run_parser_method("SELECT * FROM foo FOR", |p| p.parse_query())
        .is_err());
}

#[test]
fn parse_for_json_expect_ast() {
    assert_eq!(
        ms().verified_query("SELECT * FROM t FOR JSON PATH, ROOT('root')")
            .for_clause
            .unwrap(),
        ForClause::Json {
            for_json: ForJson::Path,
            root: Some("root".into()),
            without_array_wrapper: false,
            include_null_values: false,
        }
    );
}

#[test]
fn parse_mssql_json_object() {
    let select = ms().verified_only_select(
        "SELECT JSON_OBJECT('user_name' : USER_NAME(), LOWER(@id_key) : @id_value, 'sid' : (SELECT @@SPID) ABSENT ON NULL)",
    );
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert!(matches!(
                args[0],
                FunctionArg::ExprNamed {
                    name: Expr::Value(ValueWithSpan {
                        value: Value::SingleQuotedString(_),
                        span: _
                    }),
                    arg: FunctionArgExpr::Expr(Expr::Function(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
            assert!(matches!(
                args[1],
                FunctionArg::ExprNamed {
                    name: Expr::Function(_),
                    arg: FunctionArgExpr::Expr(Expr::Identifier(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
            assert!(matches!(
                args[2],
                FunctionArg::ExprNamed {
                    name: Expr::Value(ValueWithSpan {
                        value: Value::SingleQuotedString(_),
                        span: _
                    }),
                    arg: FunctionArgExpr::Expr(Expr::Subquery(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::AbsentOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select(
        "SELECT s.session_id, JSON_OBJECT('security_id' : s.security_id, 'login' : s.login_name, 'status' : s.status) AS info \
        FROM sys.dm_exec_sessions AS s \
        WHERE s.is_user_process = 1",
    );
    match &select.projection[1] {
        SelectItem::ExprWithAlias {
            expr:
                Expr::Function(Function {
                    args: FunctionArguments::List(FunctionArgumentList { args, .. }),
                    ..
                }),
            ..
        } => {
            assert!(matches!(
                args[0],
                FunctionArg::ExprNamed {
                    name: Expr::Value(ValueWithSpan {
                        value: Value::SingleQuotedString(_),
                        span: _
                    }),
                    arg: FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
            assert!(matches!(
                args[1],
                FunctionArg::ExprNamed {
                    name: Expr::Value(ValueWithSpan {
                        value: Value::SingleQuotedString(_),
                        span: _
                    }),
                    arg: FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
            assert!(matches!(
                args[2],
                FunctionArg::ExprNamed {
                    name: Expr::Value(ValueWithSpan {
                        value: Value::SingleQuotedString(_),
                        span: _
                    }),
                    arg: FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)),
                    operator: FunctionArgOperator::Colon
                }
            ));
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_mssql_json_array() {
    let select = ms().verified_only_select("SELECT JSON_ARRAY('a', 1, NULL, 2 NULL ON NULL)");
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert_eq!(
                &[
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (Value::SingleQuotedString("a".into())).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (number("1")).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (Value::Null).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (number("2")).with_empty_span()
                    ))),
                ],
                &args[..]
            );
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::NullOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select("SELECT JSON_ARRAY('a', 1, NULL, 2 ABSENT ON NULL)");
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert_eq!(
                &[
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (Value::SingleQuotedString("a".into())).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (number("1")).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (Value::Null).with_empty_span()
                    ))),
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        (number("2")).with_empty_span()
                    ))),
                ],
                &args[..]
            );
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::AbsentOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select("SELECT JSON_ARRAY(NULL ON NULL)");
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert!(args.is_empty());
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::NullOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select("SELECT JSON_ARRAY(ABSENT ON NULL)");
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert!(args.is_empty());
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::AbsentOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select(
        "SELECT JSON_ARRAY('a', JSON_OBJECT('name' : 'value', 'type' : 1) NULL ON NULL)",
    );
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
            ..
        }) => {
            assert_eq!(
                &FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                    (Value::SingleQuotedString("a".into())).with_empty_span()
                ))),
                &args[0]
            );
            assert!(matches!(
                args[1],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(_)))
            ));
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::NullOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select(
        "SELECT JSON_ARRAY('a', JSON_OBJECT('name' : 'value', 'type' : 1), JSON_ARRAY(1, NULL, 2 NULL ON NULL))",
    );
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, .. }),
            ..
        }) => {
            assert_eq!(
                &FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                    (Value::SingleQuotedString("a".into())).with_empty_span()
                ))),
                &args[0]
            );
            assert!(matches!(
                args[1],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(_)))
            ));
            assert!(matches!(
                args[2],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(_)))
            ));
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select("SELECT JSON_ARRAY(1, @id_value, (SELECT @@SPID))");
    match expr_from_projection(&select.projection[0]) {
        Expr::Function(Function {
            args: FunctionArguments::List(FunctionArgumentList { args, .. }),
            ..
        }) => {
            assert_eq!(
                &FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                    (number("1")).with_empty_span()
                ))),
                &args[0]
            );
            assert!(matches!(
                args[1],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(_)))
            ));
            assert!(matches!(
                args[2],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Subquery(_)))
            ));
        }
        _ => unreachable!(),
    }
    let select = ms().verified_only_select(
        "SELECT s.session_id, JSON_ARRAY(s.host_name, s.program_name, s.client_interface_name NULL ON NULL) AS info \
        FROM sys.dm_exec_sessions AS s \
        WHERE s.is_user_process = 1",
    );
    match &select.projection[1] {
        SelectItem::ExprWithAlias {
            expr:
                Expr::Function(Function {
                    args: FunctionArguments::List(FunctionArgumentList { args, clauses, .. }),
                    ..
                }),
            ..
        } => {
            assert!(matches!(
                args[0],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)))
            ));
            assert!(matches!(
                args[1],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)))
            ));
            assert!(matches!(
                args[2],
                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::CompoundIdentifier(_)))
            ));
            assert_eq!(
                &[FunctionArgumentClause::JsonNullClause(
                    JsonNullClause::NullOnNull
                )],
                &clauses[..]
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_ampersand_arobase() {
    // In SQL Server, a&@b means (a) & (@b), in PostgreSQL it means (a) &@ (b)
    ms().expr_parses_to("a&@b", "a & @b");
}

#[test]
fn parse_cast_varchar_max() {
    ms_and_generic().verified_expr("CAST('foo' AS VARCHAR(MAX))");
    ms_and_generic().verified_expr("CAST('foo' AS NVARCHAR(MAX))");
}

#[test]
fn parse_convert() {
    let sql = "CONVERT(INT, 1, 2, 3, NULL)";
    let Expr::Convert {
        is_try,
        expr,
        data_type,
        charset,
        target_before_value,
        styles,
    } = ms().verified_expr(sql)
    else {
        unreachable!()
    };
    assert!(!is_try);
    assert_eq!(Expr::value(number("1")), *expr);
    assert_eq!(Some(DataType::Int(None)), data_type);
    assert!(charset.is_none());
    assert!(target_before_value);
    assert_eq!(
        vec![
            Expr::value(number("2")),
            Expr::value(number("3")),
            Expr::Value((Value::Null).with_empty_span()),
        ],
        styles
    );

    ms().verified_expr("CONVERT(VARCHAR(MAX), 'foo')");
    ms().verified_expr("CONVERT(VARCHAR(10), 'foo')");
    ms().verified_expr("CONVERT(DECIMAL(10,5), 12.55)");

    let error_sql = "SELECT CONVERT(INT, 'foo',) FROM T";
    assert_eq!(
        ParserError::ParserError("Expected: an expression, found: )".to_owned()),
        ms().parse_sql_statements(error_sql).unwrap_err()
    );
}

#[test]
fn parse_substring_in_select() {
    let sql = "SELECT DISTINCT SUBSTRING(description, 0, 1) FROM test";
    match ms().one_statement_parses_to(
        sql,
        "SELECT DISTINCT SUBSTRING(description, 0, 1) FROM test",
    ) {
        Statement::Query(query) => {
            assert_eq!(
                Box::new(Query {
                    with: None,

                    body: Box::new(SetExpr::Select(Box::new(Select {
                        select_token: AttachedToken::empty(),
                        distinct: Some(Distinct::Distinct),
                        top: None,
                        top_before_distinct: false,
                        projection: vec![SelectItem::UnnamedExpr(Expr::Substring {
                            expr: Box::new(Expr::Identifier(Ident {
                                value: "description".to_string(),
                                quote_style: None,
                                span: Span::empty(),
                            })),
                            substring_from: Some(Box::new(Expr::Value(
                                (number("0")).with_empty_span()
                            ))),
                            substring_for: Some(Box::new(Expr::Value(
                                (number("1")).with_empty_span()
                            ))),
                            special: true,
                            shorthand: false,
                        })],
                        exclude: None,
                        into: None,
                        from: vec![TableWithJoins {
                            relation: table_from_name(ObjectName::from(vec![Ident {
                                value: "test".to_string(),
                                quote_style: None,
                                span: Span::empty(),
                            }])),
                            joins: vec![]
                        }],
                        lateral_views: vec![],
                        prewhere: None,
                        selection: None,
                        group_by: GroupByExpr::Expressions(vec![], vec![]),
                        cluster_by: vec![],
                        distribute_by: vec![],
                        sort_by: vec![],
                        having: None,
                        named_window: vec![],
                        qualify: None,
                        window_before_qualify: false,
                        value_table_mode: None,
                        connect_by: None,
                        flavor: SelectFlavor::Standard,
                    }))),
                    order_by: None,
                    limit_clause: None,
                    fetch: None,
                    locks: vec![],
                    for_clause: None,
                    settings: None,
                    format_clause: None,
                    pipe_operators: vec![],
                }),
                query
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_mssql_declare() {
    let sql = "DECLARE @foo CURSOR, @bar INT, @baz AS TEXT = 'foobar';";
    let ast = ms().parse_sql_statements(sql).unwrap();

    assert_eq!(
        vec![Statement::Declare {
            stmts: vec![
                Declare {
                    names: vec![Ident {
                        value: "@foo".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    }],
                    data_type: None,
                    assignment: None,
                    declare_type: Some(DeclareType::Cursor),
                    binary: None,
                    sensitive: None,
                    scroll: None,
                    hold: None,
                    for_query: None
                },
                Declare {
                    names: vec![Ident {
                        value: "@bar".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    }],
                    data_type: Some(Int(None)),
                    assignment: None,
                    declare_type: None,
                    binary: None,
                    sensitive: None,
                    scroll: None,
                    hold: None,
                    for_query: None
                },
                Declare {
                    names: vec![Ident {
                        value: "@baz".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    }],
                    data_type: Some(Text),
                    assignment: Some(MsSqlAssignment(Box::new(Expr::Value(
                        (SingleQuotedString("foobar".to_string())).with_empty_span()
                    )))),
                    declare_type: None,
                    binary: None,
                    sensitive: None,
                    scroll: None,
                    hold: None,
                    for_query: None
                }
            ]
        }],
        ast
    );

    let sql = "DECLARE @bar INT;SET @bar = 2;SELECT @bar * 4";
    let ast = ms().parse_sql_statements(sql).unwrap();
    assert_eq!(
        vec![
            Statement::Declare {
                stmts: vec![Declare {
                    names: vec![Ident::new("@bar"),],
                    data_type: Some(Int(None)),
                    assignment: None,
                    declare_type: None,
                    binary: None,
                    sensitive: None,
                    scroll: None,
                    hold: None,
                    for_query: None
                }]
            },
            Statement::Set(Set::SingleAssignment {
                scope: None,
                hivevar: false,
                variable: ObjectName::from(vec![Ident::new("@bar")]),
                values: vec![Expr::Value(
                    (Value::Number("2".parse().unwrap(), false)).with_empty_span()
                )],
            }),
            Statement::Query(Box::new(Query {
                with: None,
                limit_clause: None,
                fetch: None,
                locks: vec![],
                for_clause: None,
                order_by: None,
                settings: None,
                format_clause: None,
                pipe_operators: vec![],

                body: Box::new(SetExpr::Select(Box::new(Select {
                    select_token: AttachedToken::empty(),
                    distinct: None,
                    top: None,
                    top_before_distinct: false,
                    projection: vec![SelectItem::UnnamedExpr(Expr::BinaryOp {
                        left: Box::new(Expr::Identifier(Ident::new("@bar"))),
                        op: BinaryOperator::Multiply,
                        right: Box::new(Expr::Value(
                            (Value::Number("4".parse().unwrap(), false)).with_empty_span()
                        )),
                    })],
                    exclude: None,
                    into: None,
                    from: vec![],
                    lateral_views: vec![],
                    prewhere: None,
                    selection: None,
                    group_by: GroupByExpr::Expressions(vec![], vec![]),
                    cluster_by: vec![],
                    distribute_by: vec![],
                    sort_by: vec![],
                    having: None,
                    named_window: vec![],
                    window_before_qualify: false,
                    qualify: None,
                    value_table_mode: None,
                    connect_by: None,
                    flavor: SelectFlavor::Standard,
                })))
            }))
        ],
        ast
    );

    let declare_cursor_for_select =
        "DECLARE vend_cursor CURSOR FOR SELECT * FROM Purchasing.Vendor";
    let _ = ms().verified_stmt(declare_cursor_for_select);
}

#[test]
fn test_mssql_cursor() {
    let full_cursor_usage = "\
        DECLARE Employee_Cursor CURSOR FOR \
        SELECT LastName, FirstName \
        FROM AdventureWorks2022.HumanResources.vEmployee \
        WHERE LastName LIKE 'B%'; \
        \
        OPEN Employee_Cursor; \
        \
        FETCH NEXT FROM Employee_Cursor; \
        \
        WHILE @@FETCH_STATUS = 0 \
        BEGIN \
            FETCH NEXT FROM Employee_Cursor; \
        END; \
        \
        CLOSE Employee_Cursor; \
        DEALLOCATE Employee_Cursor\
    ";
    let _ = ms().statements_parse_to(full_cursor_usage, "");
}

#[test]
fn test_mssql_while_statement() {
    let while_single_statement = "WHILE 1 = 0 PRINT 'Hello World';";
    let stmt = ms().verified_stmt(while_single_statement);
    assert_eq!(
        stmt,
        Statement::While(sqlparser::ast::WhileStatement {
            while_block: ConditionalStatementBlock {
                start_token: AttachedToken(TokenWithSpan {
                    token: Token::Word(Word {
                        value: "WHILE".to_string(),
                        quote_style: None,
                        keyword: Keyword::WHILE
                    }),
                    span: Span::empty()
                }),
                condition: Some(Expr::BinaryOp {
                    left: Box::new(Expr::Value(
                        (Value::Number("1".parse().unwrap(), false)).with_empty_span()
                    )),
                    op: BinaryOperator::Eq,
                    right: Box::new(Expr::Value(
                        (Value::Number("0".parse().unwrap(), false)).with_empty_span()
                    )),
                }),
                then_token: None,
                conditional_statements: ConditionalStatements::Sequence {
                    statements: vec![Statement::Print(PrintStatement {
                        message: Box::new(Expr::Value(
                            (Value::SingleQuotedString("Hello World".to_string()))
                                .with_empty_span()
                        )),
                    })],
                }
            }
        })
    );

    let while_begin_end = "\
        WHILE @@FETCH_STATUS = 0 \
        BEGIN \
            FETCH NEXT FROM Employee_Cursor; \
        END\
    ";
    let _ = ms().verified_stmt(while_begin_end);

    let while_begin_end_multiple_statements = "\
        WHILE @@FETCH_STATUS = 0 \
        BEGIN \
            FETCH NEXT FROM Employee_Cursor; \
            PRINT 'Hello World'; \
        END\
    ";
    let _ = ms().verified_stmt(while_begin_end_multiple_statements);
}

#[test]
fn test_parse_raiserror() {
    let sql = r#"RAISERROR('This is a test', 16, 1)"#;
    let s = ms().verified_stmt(sql);
    assert_eq!(
        s,
        Statement::RaisError {
            message: Box::new(Expr::Value(
                (Value::SingleQuotedString("This is a test".to_string())).with_empty_span()
            )),
            severity: Box::new(Expr::Value(
                (Value::Number("16".parse().unwrap(), false)).with_empty_span()
            )),
            state: Box::new(Expr::Value(
                (Value::Number("1".parse().unwrap(), false)).with_empty_span()
            )),
            arguments: vec![],
            options: vec![],
        }
    );

    let sql = r#"RAISERROR('This is a test', 16, 1) WITH NOWAIT"#;
    let _ = ms().verified_stmt(sql);

    let sql = r#"RAISERROR('This is a test', 16, 1, 'ARG') WITH SETERROR, LOG"#;
    let _ = ms().verified_stmt(sql);

    let sql = r#"RAISERROR(N'This is message %s %d.', 10, 1, N'number', 5)"#;
    let _ = ms().verified_stmt(sql);

    let sql = r#"RAISERROR(N'<<%*.*s>>', 10, 1, 7, 3, N'abcde')"#;
    let _ = ms().verified_stmt(sql);

    let sql = r#"RAISERROR(@ErrorMessage, @ErrorSeverity, @ErrorState)"#;
    let _ = ms().verified_stmt(sql);
}

#[test]
fn parse_use() {
    let valid_object_names = [
        "mydb",
        "SCHEMA",
        "DATABASE",
        "CATALOG",
        "WAREHOUSE",
        "DEFAULT",
    ];
    let quote_styles = ['\'', '"'];
    for object_name in &valid_object_names {
        // Test single identifier without quotes
        assert_eq!(
            ms().verified_stmt(&format!("USE {object_name}")),
            Statement::Use(Use::Object(ObjectName::from(vec![Ident::new(
                object_name.to_string()
            )])))
        );
        for &quote in &quote_styles {
            // Test single identifier with different type of quotes
            assert_eq!(
                ms().verified_stmt(&format!("USE {quote}{object_name}{quote}")),
                Statement::Use(Use::Object(ObjectName::from(vec![Ident::with_quote(
                    quote,
                    object_name.to_string(),
                )])))
            );
        }
    }
}

#[test]
fn parse_create_table_with_valid_options() {
    let options = [
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (DISTRIBUTION = ROUND_ROBIN, PARTITION (column_a RANGE FOR VALUES (10, 11)))",
            vec![
                SqlOption::KeyValue {
                    key: Ident {
                        value: "DISTRIBUTION".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    },
                    value: Expr::Identifier(Ident {
                        value: "ROUND_ROBIN".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    })
                },
                SqlOption::Partition {
                    column_name: "column_a".into(),
                    range_direction: None,
                    for_values: vec![Expr::Value((test_utils::number("10")).with_empty_span()), Expr::Value((test_utils::number("11")).with_empty_span())] ,
                },
            ],
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (PARTITION (column_a RANGE LEFT FOR VALUES (10, 11)))",
            vec![
                SqlOption::Partition {
                        column_name: "column_a".into(),
                        range_direction: Some(PartitionRangeDirection::Left),
                        for_values: vec![
                            Expr::Value((test_utils::number("10")).with_empty_span()),
                            Expr::Value((test_utils::number("11")).with_empty_span()),
                        ],
                    }
            ],
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX)",
            vec![SqlOption::Clustered(TableOptionsClustered::ColumnstoreIndex)],
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX ORDER (column_a, column_b))",
            vec![
                SqlOption::Clustered(TableOptionsClustered::ColumnstoreIndexOrder(vec![
                    "column_a".into(),
                    "column_b".into(),
                ]))
            ],
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED INDEX (column_a ASC, column_b DESC, column_c))",
            vec![
                SqlOption::Clustered(TableOptionsClustered::Index(vec![
                        ClusteredIndex {
                            name: Ident {
                                value: "column_a".to_string(),
                                quote_style: None,
                                span: Span::empty(),
                            },
                            asc: Some(true),
                        },
                        ClusteredIndex {
                            name: Ident {
                                value: "column_b".to_string(),
                                quote_style: None,
                                span: Span::empty(),
                            },
                            asc: Some(false),
                        },
                        ClusteredIndex {
                            name: Ident {
                                value: "column_c".to_string(),
                                quote_style: None,
                                span: Span::empty(),
                            },
                            asc: None,
                        },
                    ]))
            ],
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (DISTRIBUTION = HASH(column_a, column_b), HEAP)",
            vec![
                SqlOption::KeyValue {
                    key: Ident {
                        value: "DISTRIBUTION".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    },
                    value: Expr::Function(
                        Function {
                            name: ObjectName::from(
                                vec![
                                    Ident {
                                        value: "HASH".to_string(),
                                        quote_style: None,
                                        span: Span::empty(),
                                    },
                                ],
                            ),
                            uses_odbc_syntax: false,
                            parameters: FunctionArguments::None,
                            args: FunctionArguments::List(
                                FunctionArgumentList {
                                    duplicate_treatment: None,
                                    args: vec![
                                        FunctionArg::Unnamed(
                                            FunctionArgExpr::Expr(
                                                Expr::Identifier(
                                                    Ident {
                                                        value: "column_a".to_string(),
                                                        quote_style: None,
                                                        span: Span::empty(),
                                                    },
                                                ),
                                            ),
                                        ),
                                        FunctionArg::Unnamed(
                                            FunctionArgExpr::Expr(
                                                Expr::Identifier(
                                                    Ident {
                                                        value: "column_b".to_string(),
                                                        quote_style: None,
                                                        span: Span::empty(),
                                                    },
                                                ),
                                            ),
                                        ),
                                    ],
                                    clauses: vec![],
                                },
                            ),
                            filter: None,
                            null_treatment: None,
                            over: None,
                            within_group: vec![],
                        },
                    ),
                },
                SqlOption::Ident("HEAP".into()),
            ],
         ),
    ];

    for (sql, with_options) in options {
        assert_eq!(
            ms_and_generic().verified_stmt(sql),
            Statement::CreateTable(CreateTable {
                or_replace: false,
                temporary: false,
                external: false,
                global: None,
                if_not_exists: false,
                transient: false,
                volatile: false,
                name: ObjectName::from(vec![Ident {
                    value: "mytable".to_string(),
                    quote_style: None,
                    span: Span::empty(),
                },],),
                columns: vec![
                    ColumnDef {
                        name: Ident {
                            value: "column_a".to_string(),
                            quote_style: None,
                            span: Span::empty(),
                        },
                        data_type: Int(None,),
                        options: vec![],
                    },
                    ColumnDef {
                        name: Ident {
                            value: "column_b".to_string(),
                            quote_style: None,
                            span: Span::empty(),
                        },
                        data_type: Int(None,),
                        options: vec![],
                    },
                    ColumnDef {
                        name: Ident {
                            value: "column_c".to_string(),
                            quote_style: None,
                            span: Span::empty(),
                        },
                        data_type: Int(None,),
                        options: vec![],
                    },
                ],
                constraints: vec![],
                hive_distribution: HiveDistributionStyle::NONE,
                hive_formats: Some(HiveFormat {
                    row_format: None,
                    serde_properties: None,
                    storage: None,
                    location: None,
                },),
                file_format: None,
                location: None,
                query: None,
                without_rowid: false,
                like: None,
                clone: None,
                comment: None,
                on_commit: None,
                on_cluster: None,
                primary_key: None,
                order_by: None,
                partition_by: None,
                cluster_by: None,
                clustered_by: None,
                inherits: None,
                strict: false,
                iceberg: false,
                copy_grants: false,
                enable_schema_evolution: None,
                change_tracking: None,
                data_retention_time_in_days: None,
                max_data_extension_time_in_days: None,
                default_ddl_collation: None,
                with_aggregation_policy: None,
                with_row_access_policy: None,
                with_tags: None,
                base_location: None,
                external_volume: None,
                catalog: None,
                catalog_sync: None,
                storage_serialization_policy: None,
                table_options: CreateTableOptions::With(with_options)
            })
        );
    }
}

#[test]
fn parse_nested_slash_star_comment() {
    let sql = r#"
    select
    /*
       comment level 1
       /*
          comment level 2
       */
    */
    1;
    "#;
    let canonical = "SELECT 1";
    ms().one_statement_parses_to(sql, canonical);
}

#[test]
fn parse_create_table_with_invalid_options() {
    let invalid_cases = vec![
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX ORDER ())",
            "Expected: identifier, found: )",
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE)",
            "invalid CLUSTERED sequence",
        ),
        (
            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (HEAP INDEX)",
            "Expected: ), found: INDEX",
        ),
        (

            "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (PARTITION (RANGE LEFT FOR VALUES (10, 11)))",
            "Expected: RANGE, found: LEFT",
        ),
    ];

    for (sql, expected_error) in invalid_cases {
        let res = ms_and_generic().parse_sql_statements(sql);
        assert_eq!(
            format!("sql parser error: {expected_error}"),
            res.unwrap_err().to_string()
        );
    }
}

#[test]
fn parse_create_table_with_identity_column() {
    let with_column_options = [
        (
            r#"CREATE TABLE mytable (columnA INT IDENTITY NOT NULL)"#,
            vec![
                ColumnOptionDef {
                    name: None,
                    option: ColumnOption::Identity(IdentityPropertyKind::Identity(
                        IdentityProperty {
                            parameters: None,
                            order: None,
                        },
                    )),
                },
                ColumnOptionDef {
                    name: None,
                    option: ColumnOption::NotNull,
                },
            ],
        ),
        (
            r#"CREATE TABLE mytable (columnA INT IDENTITY(1, 1) NOT NULL)"#,
            vec![
                ColumnOptionDef {
                    name: None,
                    option: ColumnOption::Identity(IdentityPropertyKind::Identity(
                        IdentityProperty {
                            parameters: Some(IdentityPropertyFormatKind::FunctionCall(
                                IdentityParameters {
                                    seed: Expr::value(number("1")),
                                    increment: Expr::value(number("1")),
                                },
                            )),
                            order: None,
                        },
                    )),
                },
                ColumnOptionDef {
                    name: None,
                    option: ColumnOption::NotNull,
                },
            ],
        ),
    ];

    for (sql, column_options) in with_column_options {
        assert_eq!(
            ms_and_generic().verified_stmt(sql),
            Statement::CreateTable(CreateTable {
                or_replace: false,
                temporary: false,
                external: false,
                global: None,
                if_not_exists: false,
                transient: false,
                volatile: false,
                iceberg: false,
                name: ObjectName::from(vec![Ident {
                    value: "mytable".to_string(),
                    quote_style: None,
                    span: Span::empty(),
                },],),
                columns: vec![ColumnDef {
                    name: Ident {
                        value: "columnA".to_string(),
                        quote_style: None,
                        span: Span::empty(),
                    },
                    data_type: Int(None,),

                    options: column_options,
                },],
                constraints: vec![],
                hive_distribution: HiveDistributionStyle::NONE,
                hive_formats: Some(HiveFormat {
                    row_format: None,
                    serde_properties: None,
                    storage: None,
                    location: None,
                },),
                file_format: None,
                location: None,
                query: None,
                without_rowid: false,
                like: None,
                clone: None,
                comment: None,
                on_commit: None,
                on_cluster: None,
                primary_key: None,
                order_by: None,
                partition_by: None,
                cluster_by: None,
                clustered_by: None,
                inherits: None,
                strict: false,
                copy_grants: false,
                enable_schema_evolution: None,
                change_tracking: None,
                data_retention_time_in_days: None,
                max_data_extension_time_in_days: None,
                default_ddl_collation: None,
                with_aggregation_policy: None,
                with_row_access_policy: None,
                with_tags: None,
                base_location: None,
                external_volume: None,
                catalog: None,
                catalog_sync: None,
                storage_serialization_policy: None,
                table_options: CreateTableOptions::None
            }),
        );
    }
}

#[test]
fn parse_true_false_as_identifiers() {
    assert_eq!(
        ms().verified_expr("true"),
        Expr::Identifier(Ident::new("true"))
    );
    assert_eq!(
        ms().verified_expr("false"),
        Expr::Identifier(Ident::new("false"))
    );
}

#[test]
fn parse_mssql_set_session_value() {
    ms().verified_stmt(
        "SET OFFSETS SELECT, FROM, ORDER, TABLE, PROCEDURE, STATEMENT, PARAM, EXECUTE ON",
    );
    ms().verified_stmt("SET IDENTITY_INSERT dbo.Tool ON");
    ms().verified_stmt("SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED");
    ms().verified_stmt("SET TRANSACTION ISOLATION LEVEL READ COMMITTED");
    ms().verified_stmt("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ");
    ms().verified_stmt("SET TRANSACTION ISOLATION LEVEL SNAPSHOT");
    ms().verified_stmt("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE");
    ms().verified_stmt("SET STATISTICS IO ON");
    ms().verified_stmt("SET STATISTICS XML ON");
    ms().verified_stmt("SET STATISTICS PROFILE ON");
    ms().verified_stmt("SET STATISTICS TIME ON");
    ms().verified_stmt("SET DATEFIRST 7");
    ms().verified_stmt("SET DATEFIRST @xxx");
    ms().verified_stmt("SET DATEFIRST @@xxx");
    ms().verified_stmt("SET DATEFORMAT dmy");
    ms().verified_stmt("SET DATEFORMAT @datevar");
    ms().verified_stmt("SET DATEFORMAT @@datevar");
    ms().verified_stmt("SET DEADLOCK_PRIORITY 'LOW'");
    ms().verified_stmt("SET DEADLOCK_PRIORITY LOW");
    ms().verified_stmt("SET DEADLOCK_PRIORITY 8");
    ms().verified_stmt("SET DEADLOCK_PRIORITY -8");
    ms().verified_stmt("SET DEADLOCK_PRIORITY @xxx");
    ms().verified_stmt("SET DEADLOCK_PRIORITY @@xxx");
    ms().verified_stmt("SET LOCK_TIMEOUT 1800");
    ms().verified_stmt("SET CONCAT_NULL_YIELDS_NULL ON");
    ms().verified_stmt("SET CURSOR_CLOSE_ON_COMMIT ON");
    ms().verified_stmt("SET FIPS_FLAGGER 'level'");
    ms().verified_stmt("SET FIPS_FLAGGER OFF");
    ms().verified_stmt("SET LANGUAGE Italian");
    ms().verified_stmt("SET QUOTED_IDENTIFIER ON");
    ms().verified_stmt("SET ARITHABORT ON");
    ms().verified_stmt("SET ARITHIGNORE OFF");
    ms().verified_stmt("SET FMTONLY ON");
    ms().verified_stmt("SET NOCOUNT OFF");
    ms().verified_stmt("SET NOEXEC ON");
    ms().verified_stmt("SET NUMERIC_ROUNDABORT ON");
    ms().verified_stmt("SET QUERY_GOVERNOR_COST_LIMIT 11");
    ms().verified_stmt("SET ROWCOUNT 4");
    ms().verified_stmt("SET ROWCOUNT @xxx");
    ms().verified_stmt("SET ROWCOUNT @@xxx");
    ms().verified_stmt("SET TEXTSIZE 11");
    ms().verified_stmt("SET ANSI_DEFAULTS ON");
    ms().verified_stmt("SET ANSI_NULL_DFLT_OFF ON");
    ms().verified_stmt("SET ANSI_NULL_DFLT_ON ON");
    ms().verified_stmt("SET ANSI_NULLS ON");
    ms().verified_stmt("SET ANSI_PADDING ON");
    ms().verified_stmt("SET ANSI_WARNINGS ON");
    ms().verified_stmt("SET FORCEPLAN ON");
    ms().verified_stmt("SET SHOWPLAN_ALL ON");
    ms().verified_stmt("SET SHOWPLAN_TEXT ON");
    ms().verified_stmt("SET SHOWPLAN_XML ON");
    ms().verified_stmt("SET IMPLICIT_TRANSACTIONS ON");
    ms().verified_stmt("SET REMOTE_PROC_TRANSACTIONS ON");
    ms().verified_stmt("SET XACT_ABORT ON");
    ms().verified_stmt("SET ANSI_NULLS, ANSI_PADDING ON");
}

#[test]
fn parse_mssql_if_else() {
    // Simple statements and blocks
    ms().verified_stmt("IF 1 = 1 SELECT '1'; ELSE SELECT '2';");
    ms().verified_stmt("IF 1 = 1 BEGIN SET @A = 1; END ELSE SET @A = 2;");
    ms().verified_stmt(
        "IF DATENAME(weekday, GETDATE()) IN (N'Saturday', N'Sunday') SELECT 'Weekend'; ELSE SELECT 'Weekday';"
    );
    ms().verified_stmt(
        "IF (SELECT COUNT(*) FROM a.b WHERE c LIKE 'x%') > 1 SELECT 'yes'; ELSE SELECT 'No';",
    );

    // Multiple statements
    let stmts = ms()
        .parse_sql_statements("DECLARE @A INT; IF 1=1 BEGIN SET @A = 1 END ELSE SET @A = 2")
        .unwrap();
    match &stmts[..] {
        [Statement::Declare { .. }, Statement::If(stmt)] => {
            assert_eq!(
                stmt.to_string(),
                "IF 1 = 1 BEGIN SET @A = 1; END ELSE SET @A = 2;"
            );
        }
        _ => panic!("Unexpected statements: {stmts:?}"),
    }
}

#[test]
fn test_mssql_if_else_span() {
    let sql = "IF 1 = 1 SELECT '1' ELSE SELECT '2'";
    let mut parser = Parser::new(&MsSqlDialect {}).try_with_sql(sql).unwrap();
    assert_eq!(
        parser.parse_statement().unwrap().span(),
        Span::new(Location::new(1, 1), Location::new(1, sql.len() as u64 + 1))
    );
}

#[test]
fn test_mssql_if_else_multiline_span() {
    let sql_line1 = "IF 1 = 1";
    let sql_line2 = "SELECT '1'";
    let sql_line3 = "ELSE SELECT '2'";
    let sql = [sql_line1, sql_line2, sql_line3].join("\n");
    let mut parser = Parser::new(&MsSqlDialect {}).try_with_sql(&sql).unwrap();
    assert_eq!(
        parser.parse_statement().unwrap().span(),
        Span::new(
            Location::new(1, 1),
            Location::new(3, sql_line3.len() as u64 + 1)
        )
    );
}

#[test]
fn test_mssql_if_statements_span() {
    // Simple statements
    let mut sql = "IF 1 = 1 SELECT '1' ELSE SELECT '2'";
    let mut parser = Parser::new(&MsSqlDialect {}).try_with_sql(sql).unwrap();
    match parser.parse_statement().unwrap() {
        Statement::If(IfStatement {
            if_block,
            else_block: Some(else_block),
            ..
        }) => {
            assert_eq!(
                if_block.span(),
                Span::new(Location::new(1, 1), Location::new(1, 20))
            );
            assert_eq!(
                else_block.span(),
                Span::new(Location::new(1, 21), Location::new(1, 36))
            );
        }
        stmt => panic!("Unexpected statement: {stmt:?}"),
    }

    // Blocks
    sql = "IF 1 = 1 BEGIN SET @A = 1; END ELSE BEGIN SET @A = 2 END";
    parser = Parser::new(&MsSqlDialect {}).try_with_sql(sql).unwrap();
    match parser.parse_statement().unwrap() {
        Statement::If(IfStatement {
            if_block,
            else_block: Some(else_block),
            ..
        }) => {
            assert_eq!(
                if_block.span(),
                Span::new(Location::new(1, 1), Location::new(1, 31))
            );
            assert_eq!(
                else_block.span(),
                Span::new(Location::new(1, 32), Location::new(1, 57))
            );
        }
        stmt => panic!("Unexpected statement: {stmt:?}"),
    }
}

#[test]
fn parse_mssql_varbinary_max_length() {
    let sql = "CREATE TABLE example (var_binary_col VARBINARY(MAX))";

    match ms_and_generic().verified_stmt(sql) {
        Statement::CreateTable(CreateTable { name, columns, .. }) => {
            assert_eq!(
                name,
                ObjectName::from(vec![Ident {
                    value: "example".to_string(),
                    quote_style: None,
                    span: Span::empty(),
                }])
            );
            assert_eq!(
                columns,
                vec![ColumnDef {
                    name: Ident::new("var_binary_col"),
                    data_type: Varbinary(Some(BinaryLength::Max)),

                    options: vec![]
                },],
            );
        }
        _ => unreachable!(),
    }

    let sql = "CREATE TABLE example (var_binary_col VARBINARY(50))";

    match ms_and_generic().verified_stmt(sql) {
        Statement::CreateTable(CreateTable { name, columns, .. }) => {
            assert_eq!(
                name,
                ObjectName::from(vec![Ident {
                    value: "example".to_string(),
                    quote_style: None,
                    span: Span::empty(),
                }])
            );
            assert_eq!(
                columns,
                vec![ColumnDef {
                    name: Ident::new("var_binary_col"),
                    data_type: Varbinary(Some(BinaryLength::IntegerLength { length: 50 })),

                    options: vec![]
                },],
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_mssql_table_identifier_with_default_schema() {
    ms().verified_stmt("SELECT * FROM mydatabase..MyTable");
}

fn ms() -> TestedDialects {
    TestedDialects::new(vec![Box::new(MsSqlDialect {})])
}

// MS SQL dialect with support for optional semi-colon statement delimiters
fn tsql() -> TestedDialects {
    TestedDialects::new_with_options(
        vec![Box::new(MsSqlDialect {})],
        ParserOptions {
            trailing_commas: false,
            unescape: true,
            require_semicolon_stmt_delimiter: false,
        },
    )
}

fn ms_and_generic() -> TestedDialects {
    TestedDialects::new(vec![Box::new(MsSqlDialect {}), Box::new(GenericDialect {})])
}

#[test]
fn parse_mssql_merge_with_output() {
    let stmt = "MERGE dso.products AS t \
        USING dsi.products AS \
        s ON s.ProductID = t.ProductID \
        WHEN MATCHED AND \
        NOT (t.ProductName = s.ProductName OR (ISNULL(t.ProductName, s.ProductName) IS NULL)) \
        THEN UPDATE SET t.ProductName = s.ProductName \
        WHEN NOT MATCHED BY TARGET \
        THEN INSERT (ProductID, ProductName) \
        VALUES (s.ProductID, s.ProductName) \
        WHEN NOT MATCHED BY SOURCE THEN DELETE \
        OUTPUT $action, deleted.ProductID INTO dsi.temp_products";
    ms_and_generic().verified_stmt(stmt);
}

#[test]
fn parse_create_trigger() {
    let create_trigger = "\
        CREATE OR ALTER TRIGGER reminder1 \
        ON Sales.Customer \
        AFTER INSERT, UPDATE \
        AS RAISERROR('Notify Customer Relations', 16, 10);\
    ";
    let create_stmt = ms().verified_stmt(create_trigger);
    assert_eq!(
        create_stmt,
        Statement::CreateTrigger {
            or_alter: true,
            or_replace: false,
            is_constraint: false,
            name: ObjectName::from(vec![Ident::new("reminder1")]),
            period: TriggerPeriod::After,
            events: vec![TriggerEvent::Insert, TriggerEvent::Update(vec![]),],
            table_name: ObjectName::from(vec![Ident::new("Sales"), Ident::new("Customer")]),
            referenced_table_name: None,
            referencing: vec![],
            trigger_object: TriggerObject::Statement,
            include_each: false,
            condition: None,
            exec_body: None,
            statements: Some(ConditionalStatements::Sequence {
                statements: vec![Statement::RaisError {
                    message: Box::new(Expr::Value(
                        (Value::SingleQuotedString("Notify Customer Relations".to_string()))
                            .with_empty_span()
                    )),
                    severity: Box::new(Expr::Value(
                        (Value::Number("16".parse().unwrap(), false)).with_empty_span()
                    )),
                    state: Box::new(Expr::Value(
                        (Value::Number("10".parse().unwrap(), false)).with_empty_span()
                    )),
                    arguments: vec![],
                    options: vec![],
                }],
            }),
            characteristics: None,
        }
    );

    let multi_statement_as_trigger = "\
        CREATE TRIGGER some_trigger ON some_table FOR INSERT \
        AS \
        DECLARE @var INT; \
        RAISERROR('Trigger fired', 10, 1);\
    ";
    let _ = ms().verified_stmt(multi_statement_as_trigger);

    let multi_statement_trigger = "\
        CREATE TRIGGER some_trigger ON some_table FOR INSERT \
        AS \
        BEGIN \
            DECLARE @var INT; \
            RAISERROR('Trigger fired', 10, 1); \
        END\
    ";
    let _ = ms().verified_stmt(multi_statement_trigger);

    let create_trigger_with_return = "\
        CREATE TRIGGER some_trigger ON some_table FOR INSERT \
        AS \
        BEGIN \
            RETURN; \
        END\
    ";
    let _ = ms().verified_stmt(create_trigger_with_return);

    let create_trigger_with_return = "\
        CREATE TRIGGER some_trigger ON some_table FOR INSERT \
        AS \
        BEGIN \
            RETURN; \
        END\
    ";
    let _ = ms().verified_stmt(create_trigger_with_return);

    let create_trigger_with_conditional = "\
        CREATE TRIGGER some_trigger ON some_table FOR INSERT \
        AS \
        BEGIN \
            IF 1 = 2 \
            BEGIN \
                RAISERROR('Trigger fired', 10, 1); \
            END; \
            RETURN; \
        END\
    ";
    let _ = ms().verified_stmt(create_trigger_with_conditional);
}

#[test]
fn parse_drop_trigger() {
    let sql_drop_trigger = "DROP TRIGGER emp_stamp;";
    let drop_stmt = ms().one_statement_parses_to(sql_drop_trigger, "");
    assert_eq!(
        drop_stmt,
        Statement::DropTrigger {
            if_exists: false,
            trigger_name: ObjectName::from(vec![Ident::new("emp_stamp")]),
            table_name: None,
            option: None,
        }
    );
}

#[test]
fn parse_print() {
    let print_string_literal = "PRINT 'Hello, world!'";
    let print_stmt = ms().verified_stmt(print_string_literal);
    assert_eq!(
        print_stmt,
        Statement::Print(PrintStatement {
            message: Box::new(Expr::Value(
                (Value::SingleQuotedString("Hello, world!".to_string())).with_empty_span()
            )),
        })
    );

    let _ = ms().verified_stmt("PRINT N'Hello, ⛄️!'");
    let _ = ms().verified_stmt("PRINT @my_variable");
}

#[test]
fn parse_mssql_grant() {
    ms().verified_stmt("GRANT SELECT ON my_table TO public, db_admin");
}

#[test]
fn parse_mssql_deny() {
    ms().verified_stmt("DENY SELECT ON my_table TO public, db_admin");
}

#[test]
fn test_tsql_no_semicolon_delimiter() {
    let sql = r#"
DECLARE @X AS NVARCHAR(MAX)='x'
DECLARE @Y AS NVARCHAR(MAX)='y'
    "#;

    let stmts = tsql().parse_sql_statements(sql).unwrap();
    assert_eq!(stmts.len(), 2);
    assert!(stmts.iter().all(|s| matches!(s, Statement::Declare { .. })));
}
