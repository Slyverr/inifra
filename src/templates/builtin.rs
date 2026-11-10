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
        manifest: include_str!("../../templates/gitignore-rust/template.toml"),
        files: &[EmbeddedFile {
            source: ".gitignore",
            contents: include_bytes!("../../templates/gitignore-rust/.gitignore"),
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
