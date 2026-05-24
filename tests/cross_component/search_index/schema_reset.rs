use super::super::support::*;

const INDEX_SENTINEL: &str = ".aghist-search-index";

#[test]
fn search_index_rebuilds_when_schema_changes() {
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }
    fs::write(
        index_dir.path().join(INDEX_SENTINEL),
        "aghist search index\n",
    )
    .unwrap();

    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }
    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty(), "rebuilt index should be queryable");
}

#[test]
fn search_index_schema_reset_refuses_missing_sentinel() {
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("schema reset should reject dirs without aghist sentinel");
    };
    assert!(
        err.to_string().contains("missing .aghist-search-index"),
        "unexpected error: {err}"
    );
    assert!(index_dir.path().join("meta.json").exists());
}

#[test]
fn search_index_schema_reset_refuses_unknown_files() {
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }
    fs::write(
        index_dir.path().join(INDEX_SENTINEL),
        "aghist search index\n",
    )
    .unwrap();
    let keep = index_dir.path().join("keep.txt");
    fs::write(&keep, "do not delete").unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("schema reset should reject dir with unknown files");
    };
    assert!(
        err.to_string()
            .contains("refusing to reset index directory"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&keep).unwrap(), "do not delete");
}

#[cfg(unix)]
#[test]
fn search_index_schema_reset_refuses_symlink_entries() {
    use std::os::unix::fs::symlink;
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }
    fs::write(
        index_dir.path().join(INDEX_SENTINEL),
        "aghist search index\n",
    )
    .unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("target.txt");
    fs::write(&target, "do not touch").unwrap();
    symlink(&target, index_dir.path().join("linked-target")).unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("schema reset should reject symlink entries");
    };
    assert!(
        err.to_string()
            .contains("refusing to reset index directory"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "do not touch");
}

#[test]
fn search_index_does_not_delete_arbitrary_meta_json() {
    let index_dir = tempfile::tempdir().unwrap();
    let meta = index_dir.path().join("meta.json");
    fs::write(&meta, r#"{"not":"tantivy"}"#).unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("arbitrary meta.json should not be treated as an aghist cache");
    };
    assert!(
        err.to_string().contains("index error"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&meta).unwrap(), r#"{"not":"tantivy"}"#);
}
