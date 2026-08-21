//! Scalar functions resolve on the paths the toolkit routes queries through.
//!
//! `sqlx-sqlite-conn-mgr/tests/function_tests.rs` covers the connection-level behavior.
//! These tests cover what the toolkit adds on top: transactions, `INSERT ... SELECT`, and a
//! query naming an attached database.
//!
//! Every test in this binary shares the process-global function registry, and each
//! database captures the registered set at its `connect`. `REGISTERED` therefore holds
//! every function this binary needs, under a name per test, and `database` forces it
//! before the first database connects.

use std::sync::{Arc, LazyLock};

use serde_json::{Value, json};
use sqlx_sqlite_conn_mgr::{AttachedMode, AttachedSpec};
use sqlx_sqlite_toolkit::{
   DatabaseWrapper, FunctionError, InvocationScope, ScalarFunction, ScalarHandler, SqlValue,
   SqlValueRef, register_function,
};
use tempfile::TempDir;

fn register(name: &str, handler: ScalarHandler) {
   register_function(ScalarFunction {
      name: name.to_string(),
      arity: 1,
      deterministic: true,
      invocation_scope: InvocationScope::DirectOnly,
      handler,
   })
   .expect("registration");
}

/// A handler that upper-cases text and passes NULL through.
fn shout() -> ScalarHandler {
   Arc::new(|args| match &args[0] {
      SqlValueRef::Text(text) => Ok(SqlValue::Text(text.to_uppercase())),
      SqlValueRef::Null => Ok(SqlValue::Null),
      _ => Err(FunctionError::new("expected text")),
   })
}

/// Every function this binary uses, one per test, registered before any database opens.
static REGISTERED: LazyLock<()> = LazyLock::new(|| {
   register("fn_tx_shout", shout());
   register("fn_attached_shout", shout());
   register(
      "fn_tx_error",
      Arc::new(|_args| Err(FunctionError::new("payload is not decodable"))),
   );
});

async fn database(name: &str) -> (DatabaseWrapper, TempDir) {
   LazyLock::force(&REGISTERED);

   let temp = TempDir::new().expect("temp dir");
   let db = DatabaseWrapper::connect(&temp.path().join(name), None)
      .await
      .expect("connect");

   (db, temp)
}

/// Runs `sql` for its effect, panicking with the SQL itself.
async fn exec(db: &DatabaseWrapper, sql: &str) {
   db.execute(sql.into(), vec![]).await.expect(sql);
}

/// Runs `sql` and returns column `name` from every row, in row order.
async fn column(db: &DatabaseWrapper, sql: &str, name: &str) -> Vec<Value> {
   db.fetch_all(sql.into(), vec![])
      .await
      .expect(sql)
      .iter()
      .map(|row| row.get(name).expect(name).clone())
      .collect()
}

#[tokio::test]
async fn resolves_inside_a_transaction_and_inside_an_insert_select() {
   let (db, _temp) = database("tx.db").await;

   exec(&db, "CREATE TABLE source (body TEXT)").await;
   exec(&db, "CREATE TABLE target (body TEXT)").await;
   exec(&db, "INSERT INTO source (body) VALUES ('alpha'), ('beta')").await;

   // Both statements run on the write connection inside one transaction: the first calls
   // the function in a VALUES list, the second inside an INSERT ... SELECT.
   db.execute_transaction(vec![
      (
         "INSERT INTO target (body) VALUES (fn_tx_shout('gamma'))",
         vec![],
      ),
      (
         "INSERT INTO target (body) SELECT fn_tx_shout(body) FROM source ORDER BY body",
         vec![],
      ),
   ])
   .execute()
   .await
   .expect("transaction");

   assert_eq!(
      column(&db, "SELECT body FROM target ORDER BY body", "body").await,
      vec![json!("ALPHA"), json!("BETA"), json!("GAMMA")]
   );
}

#[tokio::test]
async fn resolves_in_a_query_naming_an_attached_database() {
   // Each database captures the registered set at its `connect`, so the registration must
   // exist before either connect below.
   LazyLock::force(&REGISTERED);

   let temp = TempDir::new().expect("temp dir");
   let main = DatabaseWrapper::connect(&temp.path().join("main.db"), None)
      .await
      .expect("connect main");
   let other = DatabaseWrapper::connect(&temp.path().join("other.db"), None)
      .await
      .expect("connect other");

   exec(&other, "CREATE TABLE logs (msg TEXT)").await;
   exec(&other, "INSERT INTO logs (msg) VALUES ('stored')").await;

   let rows = main
      .fetch_all(
         "SELECT fn_attached_shout(msg) AS shouted FROM other.logs".into(),
         vec![],
      )
      .attach(vec![AttachedSpec {
         database: Arc::clone(other.inner()),
         schema_name: "other".to_string(),
         mode: AttachedMode::ReadOnly,
      }])
      .await
      .expect("attached read");

   assert_eq!(rows.len(), 1);
   assert_eq!(rows[0].get("shouted"), Some(&json!("STORED")));
}

#[tokio::test]
async fn a_handler_error_in_a_transaction_leaves_no_transaction_open() {
   let (db, _temp) = database("tx_error.db").await;

   exec(&db, "CREATE TABLE t (body TEXT)").await;

   let error = db
      .execute_transaction(vec![
         ("INSERT INTO t (body) VALUES ('before')", vec![]),
         ("INSERT INTO t (body) VALUES (fn_tx_error('x'))", vec![]),
      ])
      .execute()
      .await
      .expect_err("the transaction must fail");

   assert!(
      error.to_string().contains("payload is not decodable"),
      "error did not include the handler's message: {error}"
   );

   // The write pool holds one connection. If the failure leaves a transaction open, this
   // write will fail with "cannot start a transaction within a transaction".
   exec(&db, "INSERT INTO t (body) VALUES ('after')").await;

   // The failed transaction rolled back, so only the later write survives.
   assert_eq!(
      column(&db, "SELECT body FROM t", "body").await,
      vec![json!("after")]
   );
}
