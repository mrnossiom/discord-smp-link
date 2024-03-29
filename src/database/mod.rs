//! Models and triggers related to database management

use anyhow::anyhow;
use diesel::{Connection, MysqlConnection};
use diesel_async::{
	pooled_connection::deadpool::{Object, Pool},
	AsyncMysqlConnection,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub(crate) mod models;
pub(crate) mod query;
/// The automatically generated schema by `Diesel`
#[rustfmt::skip]
pub(crate) mod schema;


/// The type alias for a Postgres connection pool
pub(crate) type DatabasePool = Pool<AsyncMysqlConnection>;
/// The type alias for a Postgres connection handle
pub(crate) type DatabasePooledConnection = Object<AsyncMysqlConnection>;

/// The migrations to apply to the database
pub(crate) const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// Applies the migrations to the database
pub(crate) fn run_migrations(connection: &str) -> anyhow::Result<()> {
	let mut connection = MysqlConnection::establish(connection)?;

	let migrations_applied = connection
		.run_pending_migrations(MIGRATIONS)
		.map_err(|e| anyhow!("Could not run migrations {}", e))?;

	tracing::debug!(migrations = ?migrations_applied, "Applied migrations");

	Ok(())
}

#[allow(unused_imports)]
/// Our own prelude for database related modules
pub(crate) mod prelude {
	pub(crate) use diesel::dsl as db_dsl;
	pub(crate) use diesel::prelude::{
		AppearsOnTable, AsChangeset, BelongingToDsl, BoolExpressionMethods, BoxableExpression,
		Column, CombineDsl, Connection, DecoratableTarget, EscapeExpressionMethods, Expression,
		ExpressionMethods, GroupedBy, Identifiable, Insertable, IntoSql, JoinOnDsl, JoinTo,
		NullableExpressionMethods, OptionalExtension, QueryDsl, QuerySource, Queryable,
		QueryableByName, Selectable, SelectableExpression, SelectableHelper, Table,
		TextExpressionMethods,
	};
	pub(crate) use diesel::result::Error as DieselError;
	pub(crate) use diesel_async::{RunQueryDsl, SaveChangesDsl, UpdateAndFetchResults};
}
