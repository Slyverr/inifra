use super::catalog::{EmbeddedFile, EmbeddedPack};

pub(super) const PACKS: &[EmbeddedPack] = &[
    EmbeddedPack {
        manifest: include_str!("../../templates/readme/template.toml"),
        files: &[EmbeddedFile {
            source: "README.md.j2",
            contents: include_bytes!("../../templates/readme/README.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/agents/template.toml"),
        files: &[EmbeddedFile {
            source: "AGENTS.md.j2",
            contents: include_bytes!("../../templates/agents/AGENTS.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/contributing/template.toml"),
        files: &[EmbeddedFile {
            source: "CONTRIBUTING.md.j2",
            contents: include_bytes!("../../templates/contributing/CONTRIBUTING.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/changelog/template.toml"),
        files: &[EmbeddedFile {
            source: "CHANGELOG.md",
            contents: include_bytes!("../../templates/changelog/CHANGELOG.md"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/editorconfig/template.toml"),
        files: &[EmbeddedFile {
            source: ".editorconfig",
            contents: include_bytes!("../../templates/editorconfig/.editorconfig"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/docs-architecture/template.toml"),
        files: &[EmbeddedFile {
            source: "architecture.md.j2",
            contents: include_bytes!("../../templates/docs-architecture/architecture.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/docs-development/template.toml"),
        files: &[EmbeddedFile {
            source: "development.md.j2",
            contents: include_bytes!("../../templates/docs-development/development.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/github-issues/template.toml"),
        files: &[
            EmbeddedFile {
                source: "bug-report.yml",
                contents: include_bytes!("../../templates/github-issues/bug-report.yml"),
            },
            EmbeddedFile {
                source: "feature-request.yml",
                contents: include_bytes!("../../templates/github-issues/feature-request.yml"),
            },
        ],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/github-pull-request/template.toml"),
        files: &[EmbeddedFile {
            source: "pull_request_template.md",
            contents: include_bytes!(
                "../../templates/github-pull-request/pull_request_template.md"
            ),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/security/template.toml"),
        files: &[EmbeddedFile {
            source: "SECURITY.md.j2",
            contents: include_bytes!("../../templates/security/SECURITY.md.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/license-mit/template.toml"),
        files: &[EmbeddedFile {
            source: "LICENSE.j2",
            contents: include_bytes!("../../templates/license-mit/LICENSE.j2"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/license-apache/template.toml"),
        files: &[EmbeddedFile {
            source: "LICENSE",
            contents: include_bytes!("../../templates/license-apache/LICENSE"),
        }],
    },
];
