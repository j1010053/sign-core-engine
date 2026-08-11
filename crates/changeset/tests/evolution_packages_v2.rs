use conlang_changeset::evolution::{
    Edge, EvolutionError, EvolutionGraph, Nativization, PersistedNode,
};
use conlang_changeset::{change_set_prelude_with_packages, ReplayError};
use conlang_language::{
    LanguageDocument, LibraryCatalog, PackageId, PackageRequirement, PackageResolver,
    PackageSource, PackageSources, PackageSpec, ResolvedPackages,
};

const ROOT: &str = r#"sign probe:
    belongs EvolutionExternalMarker
    entrenchment = 0.5
"#;

fn package(extra_code: &str) -> PackageSources {
    PackageSources {
        config: r#"schema = 2
id = "catalog:evolution-probe"
version = "1.0.0"
layer = "reference"
capabilities = ["traits"]
exports = "config/exports.tsv"
requires = []
code = ["code/ontology.lang"]
"#
        .to_owned(),
        exports: "stable_id\tkind\talias\ncatalog:evolution-probe:Marker\ttrait\tEvolutionExternalMarker\n"
            .to_owned(),
        code: format!("trait EvolutionExternalMarker:\n{extra_code}"),
        source: PackageSource::Injected("evolution-v2-test".to_owned()),
        ..PackageSources::default()
    }
}

fn resolved_packages(extra_code: &str) -> ResolvedPackages {
    let catalog = LibraryCatalog::with_packages([package(extra_code)]).expect("valid package");
    catalog
        .resolve(&PackageSpec::default().with_root(PackageRequirement::exact(
            PackageId::new("catalog", "evolution-probe"),
            "1.0.0",
        )))
        .expect("resolved package snapshot")
}

fn persisted(graph: &EvolutionGraph) -> Vec<PersistedNode> {
    graph
        .ids()
        .map(|id| {
            let node = graph.node(id).expect("id came from graph");
            PersistedNode {
                id: id.clone(),
                parents: node.parents().to_vec(),
                snapshot: node.snapshot().clone(),
                nativization: node.nativization(),
                label: node.label().map(str::to_owned),
            }
        })
        .collect()
}

#[test]
fn external_package_snapshot_survives_commit_and_restore() {
    let packages = resolved_packages("");
    let lock_digest = packages.lock_digest();
    let root = LanguageDocument::import_new_root(ROOT, "evo:root").expect("root");
    let mut changeset =
        change_set_prelude_with_packages(&root, &packages, "evo:next").expect("prelude");
    changeset.push_str("\n    statement 0:\n        entrench(sign(\"probe\"), delta: 0.1)\n");

    let mut graph = EvolutionGraph::new_with_packages(packages.clone());
    assert_eq!(
        graph.packages().map(ResolvedPackages::lock_digest),
        Some(lock_digest.clone())
    );
    let root_id = graph.add_root(root).expect("root added");
    let committed = graph
        .commit(
            vec![Edge::trunk(root_id, changeset)],
            Nativization::None,
            Some("external package commit".to_owned()),
        )
        .expect("commit must use the injected external package");
    let committed_entrenchment = graph
        .snapshot(&committed)
        .expect("committed snapshot")
        .language()
        .sign_named("probe")
        .expect("probe")
        .entrenchment()
        .expect("entrenchment");
    assert!((committed_entrenchment - 0.6).abs() < f64::EPSILON);

    let records = persisted(&graph);
    let changed_packages = resolved_packages("trait DigestChangingMarker:\n");
    let error = EvolutionGraph::restore_with_packages(changed_packages, records.clone())
        .expect_err("different package bytes must not verify the stored ChangeSet");
    assert!(
        matches!(
            error,
            EvolutionError::Replay(ReplayError::LibraryLockMismatch(_))
        ),
        "{error:?}"
    );

    let restored = EvolutionGraph::restore_with_packages(packages, records)
        .expect("restore must replay with the original resolved snapshot");
    assert_eq!(
        restored.packages().map(ResolvedPackages::lock_digest),
        Some(lock_digest)
    );
    assert_eq!(restored.len(), graph.len());
    assert_eq!(
        restored
            .snapshot(&committed)
            .expect("restored child")
            .source(),
        graph.snapshot(&committed).expect("original child").source()
    );
    restored.verify_all().expect("restored graph passes fsck");
}
