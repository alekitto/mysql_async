// Copyright (c) 2016 Anatoly Ikorsky
//
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. All files in the project carrying such notice may not be copied,
// modified, or distributed except according to those terms.

//! Tokio based asynchronous MySql client library for The Rust Programming Language.
//!
//! # Installation
//!
//! The library is hosted on [crates.io](https://crates.io/crates/mysql_async/).
//!
//! ```toml
//! [dependencies]
//! mysql_async = "<desired version>"
//! ```
//!
//! # Example
//!
//! ```rust
//! # use mysql_async::{Result, test_misc::get_opts};
//! use mysql_async::prelude::*;
//! # use std::env;
//!
//! #[derive(Debug, PartialEq, Eq, Clone)]
//! struct Payment {
//!     customer_id: i32,
//!     amount: i32,
//!     account_name: Option<String>,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let payments = vec![
//!         Payment { customer_id: 1, amount: 2, account_name: None },
//!         Payment { customer_id: 3, amount: 4, account_name: Some("foo".into()) },
//!         Payment { customer_id: 5, amount: 6, account_name: None },
//!         Payment { customer_id: 7, amount: 8, account_name: None },
//!         Payment { customer_id: 9, amount: 10, account_name: Some("bar".into()) },
//!     ];
//!
//!     let database_url = /* ... */
//!     # get_opts();
//!
//!     let pool = mysql_async::Pool::new(database_url);
//!     let mut conn = pool.get_conn().await?;
//!
//!     // Create a temporary table
//!     r"CREATE TEMPORARY TABLE payment (
//!         customer_id int not null,
//!         amount int not null,
//!         account_name text
//!     )".ignore(&mut conn).await?;
//!
//!     // Save payments
//!     r"INSERT INTO payment (customer_id, amount, account_name)
//!       VALUES (:customer_id, :amount, :account_name)"
//!         .with(payments.iter().map(|payment| params! {
//!             "customer_id" => payment.customer_id,
//!             "amount" => payment.amount,
//!             "account_name" => payment.account_name.as_ref(),
//!         }))
//!         .batch(&mut conn)
//!         .await?;
//!
//!     // Load payments from the database. Type inference will work here.
//!     let loaded_payments = "SELECT customer_id, amount, account_name FROM payment"
//!         .with(())
//!         .map(&mut conn, |(customer_id, amount, account_name)| Payment { customer_id, amount, account_name })
//!         .await?;
//!
//!     // Dropped connection will go to the pool
//!     drop(conn);
//!
//!     // The Pool must be disconnected explicitly because
//!     // it's an asynchronous operation.
//!     pool.disconnect().await?;
//!
//!     assert_eq!(loaded_payments, payments);
//!
//!     // the async fn returns Result, so
//!     Ok(())
//! }
//! ```
//!
//! # Pool
//!
//! The [`Pool`] structure is an asynchronous connection pool.
//!
//! Please note:
//!
//! * [`Pool`] is a smart pointer – each clone will point to the same pool instance.
//! * [`Pool`] is `Send + Sync + 'static` – feel free to pass it around.
//! * use [`Pool::disconnect`] to gracefuly close the pool.
//! * [`Pool::new`] is lazy and won't assert server availability.
//!
//! # Transaction
//!
//! [`Conn::start_transaction`] is a wrapper, that starts with `START TRANSACTION`
//! and ends with `COMMIT` or `ROLLBACK`.
//!
//! Dropped transaction will be implicitly rolled back if it wasn't explicitly
//! committed or rolled back. This behaviour will be triggered by a pool
//! or by the next query.
//!
//! API won't allow you to run nested transactions because some statements causes
//! an implicit commit (`START TRANSACTION` is one of them), so this behavior
//! is chosen as less error prone.
//!
//! # `Value`
//!
//! This enumeration represents the raw value of a MySql cell. Library offers conversion between
//! `Value` and different rust types via `FromValue` trait described below.
//!
//! ## `FromValue` trait
//!
//! This trait is reexported from **mysql_common** create. Please refer to its
//! [crate docs](https://docs.rs/mysql_common) for the list of supported conversions.
//!
//! Trait offers conversion in two flavours:
//!
//! *   `from_value(Value) -> T` - convenient, but panicking conversion.
//!
//!     Note, that for any variant of `Value` there exist a type, that fully covers its domain,
//!     i.e. for any variant of `Value` there exist `T: FromValue` such that `from_value` will never
//!     panic. This means, that if your database schema is known, than it's possible to write your
//!     application using only `from_value` with no fear of runtime panic.
//!
//!     Also note, that some convertions may fail even though the type seem sufficient,
//!     e.g. in case of invalid dates (see [sql mode](https://dev.mysql.com/doc/refman/8.0/en/sql-mode.html)).
//!
//! *   `from_value_opt(Value) -> Option<T>` - non-panicking, but less convenient conversion.
//!
//!     This function is useful to probe conversion in cases, where source database schema
//!     is unknown.
//!
//! # MySql query protocols
//!
//! ## Text protocol
//!
//! MySql text protocol is implemented in the set of `Queryable::query*` methods
//! and in the [`prelude::Query`] trait if query is [`prelude::AsQuery`].
//! It's useful when your query doesn't have parameters.
//!
//! **Note:** All values of a text protocol result set will be encoded as strings by the server,
//! so `from_value` conversion may lead to additional parsing costs.
//!
//! ## Binary protocol and prepared statements.
//!
//! MySql binary protocol is implemented in the set of `exec*` methods,
//! defined on the [`prelude::Queryable`] trait and in the [`prelude::Query`]
//! trait if query is [`QueryWithParams`]. Prepared statements is the only way to
//! pass rust value to the MySql server. MySql uses `?` symbol as a parameter placeholder.
//!
//! **Note:** it's only possible to use parameters where a single MySql value
//! is expected, i.e. you can't execute something like `SELECT ... WHERE id IN ?`
//! with a vector as a parameter. You'll need to build a query that looks like
//! `SELECT ... WHERE id IN (?, ?, ...)` and to pass each vector element as
//! a parameter.
//!
//! # LOCAL INFILE Handlers
//!
//! **Warning:** You should be aware of [Security Considerations for LOAD DATA LOCAL][1].
//!
//! There are two flavors of LOCAL INFILE handlers – _global_ and _local_.
//!
//! I case of a LOCAL INFILE request from the server the driver will try to find a handler for it:
//!
//! 1.  It'll try to use _local_ handler installed on the connection, if any;
//! 2.  It'll try to use _global_ handler, specified via [`OptsBuilder::local_infile_handler`],
//!     if any;
//! 3.  It will emit [`LocalInfileError::NoHandler`] if no handlers found.
//!
//! The purpose of a handler (_local_ or _global_) is to return [`InfileData`].
//!
//! ## _Global_ LOCAL INFILE handler
//!
//! See [`prelude::GlobalHandler`].
//!
//! Simply speaking the _global_ handler is an async function that takes a file name (as `&[u8]`)
//! and returns `Result<InfileData>`.
//!
//! You can set it up using [`OptsBuilder::local_infile_handler`]. Server will use it if there is no
//! _local_ handler installed for the connection. This handler might be called multiple times.
//!
//! Examles:
//!
//! 1.  [`WhiteListFsHandler`] is a _global_ handler.
//! 2.  Every `T: Fn(&[u8]) -> BoxFuture<'static, Result<InfileData, LocalInfileError>>`
//!     is a _global_ handler.
//!
//! ## _Local_ LOCAL INFILE handler.
//!
//! Simply speaking the _local_ handler is a future, that returns `Result<InfileData>`.
//!
//! This is a one-time handler – it's consumed after use. You can set it up using
//! [`Conn::set_infile_handler`]. This handler have priority over _global_ handler.
//!
//! Worth noting:
//!
//! 1.  `impl Drop for Conn` will clear _local_ handler, i.e. handler will be removed when
//!     connection is returned to a `Pool`.
//! 2.  [`Conn::reset`] will clear _local_ handler.
//!
//! Example:
//!
//! ```rust
//! # use mysql_async::{prelude::*, test_misc::get_opts, OptsBuilder, Result, Error};
//! # use futures_util::future::FutureExt;
//! # use futures_util::stream::{self, StreamExt};
//! # use bytes::Bytes;
//! # use std::env;
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! #
//! # let database_url = get_opts();
//! let pool = mysql_async::Pool::new(database_url);
//!
//! let mut conn = pool.get_conn().await?;
//! "CREATE TEMPORARY TABLE tmp (id INT, val TEXT)".ignore(&mut conn).await?;
//!
//! // We are going to call `LOAD DATA LOCAL` so let's setup a one-time handler.
//! conn.set_infile_handler(async move {
//!     // We need to return a stream of `io::Result<Bytes>`
//!     Ok(stream::iter([Bytes::from("1,a\r\n"), Bytes::from("2,b\r\n3,c")]).map(Ok).boxed())
//! });
//!
//! let result = r#"LOAD DATA LOCAL INFILE 'whatever'
//!     INTO TABLE `tmp`
//!     FIELDS TERMINATED BY ',' ENCLOSED BY '\"'
//!     LINES TERMINATED BY '\r\n'"#.ignore(&mut conn).await;
//!
//! match result {
//!     Ok(()) => (),
//!     Err(Error::Server(ref err)) if err.code == 1148 => {
//!         // The used command is not allowed with this MySQL version
//!         return Ok(());
//!     },
//!     Err(Error::Server(ref err)) if err.code == 3948 => {
//!         // Loading local data is disabled;
//!         // this must be enabled on both the client and the server
//!         return Ok(());
//!     }
//!     e @ Err(_) => e.unwrap(),
//! }
//!
//! // Now let's verify the result
//! let result: Vec<(u32, String)> = conn.query("SELECT * FROM tmp ORDER BY id ASC").await?;
//! assert_eq!(
//!     result,
//!     vec![(1, "a".into()), (2, "b".into()), (3, "c".into())]
//! );
//!
//! drop(conn);
//! pool.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! [1]: https://dev.mysql.com/doc/refman/8.0/en/load-data-local-security.html
//!
//! # Testing
//!
//! Tests uses followin environment variables:
//! * `DATABASE_URL` – defaults to `mysql://root:password@127.0.0.1:3307/mysql`
//! * `COMPRESS` – set to `1` or `true` to enable compression for tests
//! * `SSL` – set to `1` or `true` to enable TLS for tests
//!
//! You can run a test server using doker. Please note that params related
//! to max allowed packet, local-infile and binary logging are required
//! to properly run tests (please refer to `azure-pipelines.yml`):
//!
//! ```sh
//! docker run -d --name container \
//!     -v `pwd`:/root \
//!     -p 3307:3306 \
//!     -e MYSQL_ROOT_PASSWORD=password \
//!     mysql:8.0 \
//!     --max-allowed-packet=36700160 \
//!     --local-infile \
//!     --log-bin=mysql-bin \
//!     --log-slave-updates \
//!     --gtid_mode=ON \
//!     --enforce_gtid_consistency=ON \
//!     --server-id=1
//! ```
//!

#![recursion_limit = "1024"]
#![cfg_attr(feature = "nightly", feature(test))]

#[cfg(feature = "nightly")]
extern crate test;

pub use mysql_common::{constants as consts, params};

use std::sync::Arc;

mod buffer_pool;

#[macro_use]
mod macros;
mod conn;
mod connection_like;
/// Errors used in this crate
mod error;
mod io;
mod local_infile_handler;
mod opts;
mod query;
mod queryable;

type BoxFuture<'a, T> = futures_core::future::BoxFuture<'a, Result<T>>;

static BUFFER_POOL: once_cell::sync::Lazy<Arc<crate::buffer_pool::BufferPool>> =
    once_cell::sync::Lazy::new(|| Default::default());

#[doc(inline)]
pub use self::conn::{binlog_stream::BinlogStream, Conn};

#[doc(inline)]
pub use self::conn::pool::Pool;

#[doc(inline)]
pub use self::error::{
    DriverError, Error, IoError, LocalInfileError, ParseError, Result, ServerError, UrlError,
};

#[doc(inline)]
pub use self::query::QueryWithParams;

#[doc(inline)]
pub use self::queryable::transaction::IsolationLevel;

#[doc(inline)]
pub use self::opts::{
    Opts, OptsBuilder, PoolConstraints, PoolOpts, SslOpts, DEFAULT_INACTIVE_CONNECTION_TTL,
    DEFAULT_POOL_CONSTRAINTS, DEFAULT_STMT_CACHE_SIZE, DEFAULT_TTL_CHECK_INTERVAL,
};

#[doc(inline)]
pub use self::local_infile_handler::{builtin::WhiteListFsHandler, InfileData};

#[doc(inline)]
pub use mysql_common::packets::{
    binlog_request::BinlogRequest,
    session_state_change::{
        Gtids, Schema, SessionStateChange, SystemVariable, TransactionCharacteristics,
        TransactionState, Unsupported,
    },
    BinlogDumpFlags, Column, Interval, OkPacket, SessionStateInfo, Sid,
};

pub mod binlog {
    #[doc(inline)]
    pub use mysql_common::binlog::consts::*;

    #[doc(inline)]
    pub use mysql_common::binlog::{events, jsonb, jsondiff, row, value};
}

#[doc(inline)]
pub use mysql_common::proto::codec::Compression;

#[doc(inline)]
pub use mysql_common::row::Row;

#[doc(inline)]
pub use mysql_common::params::Params;

#[doc(inline)]
pub use mysql_common::value::Value;

#[doc(inline)]
pub use mysql_common::row::convert::{from_row, from_row_opt, FromRowError};

#[doc(inline)]
pub use mysql_common::value::convert::{from_value, from_value_opt, FromValueError};

#[doc(inline)]
pub use mysql_common::value::json::{Deserialized, Serialized};

#[doc(inline)]
pub use self::queryable::query_result::{result_set_stream::ResultSetStream, QueryResult};

#[doc(inline)]
pub use self::queryable::transaction::{Transaction, TxOpts};

#[doc(inline)]
pub use self::queryable::{BinaryProtocol, TextProtocol};

#[doc(inline)]
pub use self::queryable::stmt::Statement;

/// Futures used in this crate
pub mod futures {
    pub use crate::conn::pool::futures::{DisconnectPool, GetConn};
}

/// Traits used in this crate
pub mod prelude {
    #[doc(inline)]
    pub use crate::local_infile_handler::GlobalHandler;
    #[doc(inline)]
    pub use crate::query::{BatchQuery, Query, WithParams};
    #[doc(inline)]
    pub use crate::queryable::Queryable;
    #[doc(inline)]
    pub use mysql_common::row::convert::FromRow;
    #[doc(inline)]
    pub use mysql_common::value::convert::{ConvIr, FromValue, ToValue};

    /// Everything that is a statement.
    ///
    /// ```no_run
    /// # use std::{borrow::Cow, sync::Arc};
    /// # use mysql_async::{Statement, prelude::StatementLike};
    /// fn type_is_a_stmt<T: StatementLike>() {}
    ///
    /// type_is_a_stmt::<Cow<'_, str>>();
    /// type_is_a_stmt::<&'_ str>();
    /// type_is_a_stmt::<String>();
    /// type_is_a_stmt::<Box<str>>();
    /// type_is_a_stmt::<Arc<str>>();
    /// type_is_a_stmt::<Statement>();
    ///
    /// fn ref_to_a_clonable_stmt_is_also_a_stmt<T: StatementLike + Clone>() {
    ///     type_is_a_stmt::<&T>();
    /// }
    /// ```
    pub trait StatementLike: crate::queryable::stmt::StatementLike {}
    impl<T: crate::queryable::stmt::StatementLike> StatementLike for T {}

    /// Everything that is a connection.
    ///
    /// Note that you could obtain a `'static` connection by giving away `Conn` or `Pool`.
    pub trait ToConnection<'a, 't: 'a>: crate::connection_like::ToConnection<'a, 't> {}
    // explicitly implemented because of rusdoc
    impl<'a> ToConnection<'a, 'static> for &'a crate::Pool {}
    impl<'a> ToConnection<'static, 'static> for crate::Pool {}
    impl ToConnection<'static, 'static> for crate::Conn {}
    impl<'a> ToConnection<'a, 'static> for &'a mut crate::Conn {}
    impl<'a, 't> ToConnection<'a, 't> for &'a mut crate::Transaction<'t> {}

    /// Trait for protocol markers [`crate::TextProtocol`] and [`crate::BinaryProtocol`].
    pub trait Protocol: crate::queryable::Protocol {}
    impl Protocol for crate::BinaryProtocol {}
    impl Protocol for crate::TextProtocol {}

    pub use mysql_common::params;
}

#[doc(hidden)]
pub mod test_misc {
    use lazy_static::lazy_static;

    use std::env;

    use crate::opts::{Opts, OptsBuilder, SslOpts};

    #[allow(dead_code)]
    #[allow(unreachable_code)]
    fn error_should_implement_send_and_sync() {
        fn _dummy<T: Send + Sync + Unpin>(_: T) {}
        _dummy(panic!());
    }

    lazy_static! {
        pub static ref DATABASE_URL: String = {
            if let Ok(url) = env::var("DATABASE_URL") {
                let opts = Opts::from_url(&url).expect("DATABASE_URL invalid");
                if opts
                    .db_name()
                    .expect("a database name is required")
                    .is_empty()
                {
                    panic!("database name is empty");
                }
                url
            } else {
                "mysql://root:password@127.0.0.1:3307/mysql".into()
            }
        };
    }

    pub fn get_opts() -> OptsBuilder {
        let mut builder = OptsBuilder::from_opts(Opts::from_url(&**DATABASE_URL).unwrap());
        if test_ssl() {
            let ssl_opts = SslOpts::default()
                .with_danger_skip_domain_validation(true)
                .with_danger_accept_invalid_certs(true);
            builder = builder.prefer_socket(false).ssl_opts(ssl_opts);
        }
        if test_compression() {
            builder = builder.compression(crate::Compression::default());
        }
        builder
    }

    pub fn test_compression() -> bool {
        ["true", "1"].contains(&&*env::var("COMPRESS").unwrap_or_default())
    }

    pub fn test_ssl() -> bool {
        ["true", "1"].contains(&&*env::var("SSL").unwrap_or_default())
    }
}
