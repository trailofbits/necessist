use crate::{util, Outcome, Span};
use anyhow::{bail, Context, Result};
use diesel::prelude::*;
use diesel::{insert_into, sql_query, sqlite::SqliteConnection};
use git2::{Oid, Repository, RepositoryOpenFlags};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    ffi::OsStr,
    fmt::Debug,
    include_str,
    iter::empty,
    path::{Path, PathBuf},
    rc::Rc,
};

pub(crate) struct Sqlite {
    connection: SqliteConnection,
    remote: Option<Remote>,
}

struct Remote {
    pub repository: Repository,
    pub url: String,
    pub oid: Oid,
}

diesel::table! {
    removal (span) {
        span -> Text,
        text -> Text,
        outcome -> Text,
        url -> Text,
    }
}

#[derive(Debug, Insertable, Queryable)]
#[diesel(table_name = removal)]
struct Removal {
    pub span: String,
    pub text: String,
    pub outcome: String,
    pub url: String,
}

impl Removal {
    fn into_internal_removal(self, root: &Rc<PathBuf>) -> Result<crate::Removal> {
        let Removal {
            span,
            text,
            outcome,
            url: _,
        } = self;
        let span = Span::parse(root, &span)?;
        let outcome = outcome.parse::<Outcome>()?;
        Ok(crate::Removal {
            span,
            text,
            outcome,
        })
    }
}

pub(crate) fn init(
    root: &Path,
    must_not_exist: bool,
    reset: bool,
) -> Result<(Sqlite, Vec<crate::Removal>)> {
    let root = Rc::new(root.to_path_buf());
    let path_buf = root.join("necessist.db");

    let exists = path_buf.try_exists()?;

    if must_not_exist && exists {
        bail!(
            "Found an sqlite database at {:?}; please pass either --reset or --resume",
            path_buf
        );
    }

    let database_url = format!("sqlite://{}", path_buf.to_string_lossy());
    let mut connection = SqliteConnection::establish(&database_url)?;

    if reset && exists {
        let sql = include_str!("drop_table_removal.sql");
        sql_query(sql)
            .execute(&mut connection)
            .with_context(|| "Failed to drop sqlite database")?;
    }

    let removals = if reset || !exists {
        let sql = include_str!("create_table_removal.sql");
        sql_query(sql)
            .execute(&mut connection)
            .with_context(|| "Failed to create sqlite database")?;
        Vec::new()
    } else {
        let removals = removal::table.load::<Removal>(&mut connection)?;
        removals
            .into_iter()
            .map(|removal| removal.into_internal_removal(&root))
            .collect::<Result<Vec<_>>>()?
    };

    let remote = Repository::open_ext(&*root, RepositoryOpenFlags::empty(), empty::<&OsStr>())
        .ok()
        .and_then(|repository| {
            let url_oid = repository
                .find_remote("origin")
                .ok()
                .and_then(|origin| origin.url().map(str::to_owned))
                .and_then(|url| repository.refname_to_id("HEAD").ok().map(|oid| (url, oid)));
            url_oid.map(|(url, oid)| Remote {
                repository,
                url,
                oid,
            })
        });

    Ok((Sqlite { connection, remote }, removals))
}

pub(crate) fn insert(sqlite: &mut Sqlite, removal: &crate::Removal) -> Result<()> {
    let crate::Removal {
        span,
        text,
        outcome,
    } = removal;

    let removal = Removal {
        span: span.to_string(),
        text: text.clone(),
        outcome: outcome.to_string(),
        url: sqlite
            .remote
            .as_ref()
            .map(|remote| url_from_span(remote, span))
            .unwrap_or_default(),
    };

    insert_into(removal::table)
        .values(&removal)
        .execute(&mut sqlite.connection)
        .with_context(|| format!("Failed to insert {removal:?}"))?;

    Ok(())
}

lazy_static! {
    static ref SSH_RE: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(r"^[^@]*@([^:]*):(.*)$").unwrap()
    };
}

fn url_from_span(remote: &Remote, span: &Span) -> String {
    let base_url = remote.url.strip_suffix(".git").unwrap_or(&remote.url);

    let base_url = if let Some(captures) = SSH_RE.captures(base_url) {
        assert!(captures.len() == 3);
        format!("https://{}/{}", &captures[1], &captures[2])
    } else {
        base_url.to_owned()
    };

    #[allow(clippy::unwrap_used)]
    let path = remote
        .repository
        .workdir()
        .and_then(|path| util::strip_prefix(&span.source_file, path).ok())
        .unwrap();

    base_url
        + "/blob/"
        + &remote.oid.to_string()
        + "/"
        + &path.to_string_lossy()
        + "#L"
        + &span.start.line.to_string()
        + "-L"
        + &span.end.line.to_string()
}
