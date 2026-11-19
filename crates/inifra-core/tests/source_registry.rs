use inifra_core::source_registry::{
    GitSource, SourceRegistry, validate_git_reference, validate_git_url, validate_revision,
};
use std::fs;
use tempfile::tempdir;

fn sample_source() -> GitSource {
    GitSource {
        url: "https://github.com/example/team-templates.git".into(),
        reference: "refs/heads/main".into(),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
    }
}

#[test]
fn stores_and_loads_a_named_pinned_source() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("sources.toml");
    let mut registry = SourceRegistry::default();
    registry.insert("team", sample_source()).unwrap();

    registry.save(&path).unwrap();

    assert_eq!(SourceRegistry::load(&path).unwrap(), registry);
}

#[test]
fn absent_registry_is_empty() {
    let directory = tempdir().unwrap();

    assert!(
        SourceRegistry::load(&directory.path().join("sources.toml"))
            .unwrap()
            .sources
            .is_empty()
    );
}

#[test]
fn rejects_unsupported_schema_and_invalid_source_entries() {
    for contents in [
        "schema_version = 2\n",
        "schema_version = 1\nunknown = true\n",
        "schema_version = 1\n[sources.bad_name]\nurl = 'https://example.com/repo.git'\nreference = 'refs/heads/main'\nrevision = '0123456789abcdef0123456789abcdef01234567'\n",
    ] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("sources.toml");
        fs::write(&path, contents).unwrap();

        assert!(SourceRegistry::load(&path).is_err(), "{contents}");
    }
}

#[test]
fn rejects_credentials_and_ambiguous_ref_forms() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("sources.toml");
    let mut registry = SourceRegistry::default();
    let mut source = sample_source();
    source.url = "https://user:token@example.com/repo.git".into();

    assert!(registry.insert("team", source).is_err());
    assert!(registry.insert("team", sample_source()).is_ok());
    assert!(registry.insert("team", sample_source()).is_err());
    assert_eq!(registry.sources["team"].revision.len(), 40);
    assert!(
        registry.sources["team"]
            .reference
            .starts_with("refs/heads/")
    );
    assert!(registry.save(&path).is_ok());
}

#[test]
fn validates_supported_git_urls_refs_and_full_revisions() {
    for url in [
        "https://github.com/example/repo.git",
        "ssh://git@github.com/example/repo.git",
        "git@github.com:example/repo.git",
        "file:///tmp/local-repo.git",
    ] {
        assert!(validate_git_url(url).is_ok(), "{url}");
    }
    for url in [
        "https://user:token@example.com/repo.git",
        "https://bad host/repo.git",
        "-uhttps://example.com/repo.git",
        "ssh://-oProxyCommand=echo/repo.git",
        "git@-oProxyCommand=echo:repo.git",
        "git://example.com/repo.git",
        "git@github.com:team name/repo.git",
        "file://relative/repo.git",
    ] {
        assert!(validate_git_url(url).is_err(), "{url}");
    }

    assert!(validate_git_reference("HEAD").is_ok());
    assert!(validate_git_reference("refs/heads/main").is_ok());
    assert!(validate_git_reference("refs/tags/v1.0.0").is_ok());
    assert!(validate_git_reference("main").is_err());
    assert!(validate_git_reference("refs/heads/../main").is_err());
    assert!(validate_revision(&sample_source().revision).is_ok());
    assert!(validate_revision(&"a".repeat(64)).is_ok());
    assert!(validate_revision("0123").is_err());
}

#[test]
fn replaces_and_removes_pins_without_changing_source_identity() {
    let mut registry = SourceRegistry::default();
    registry.insert("team", sample_source()).unwrap();
    let mut updated = sample_source();
    updated.revision = "abcdef0123456789abcdef0123456789abcdef01".into();

    registry.replace("team", updated.clone()).unwrap();

    assert_eq!(registry.sources["team"], updated);
    assert_eq!(registry.remove("team").unwrap(), updated);
    assert!(registry.sources.is_empty());
}
