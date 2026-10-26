# Package exports and qualified names

Implementation contract (2026-10-16, owner request; decision I31). This supersedes the
Phase A bare-name restriction in `architecture/package_loader_v2.md`.

## Engine profile

- Representation: Rust Language AST; traits, Signs and Paradigms, typed
  applications and four-dimensional content. Source plus identity sidecar is
  authoritative; compiled artifacts are detached projections.
- Changes: identity-addressed Primitive Edits and sequential ChangeSet
  statements, replayed against an immutable resolved package snapshot.
- Boundaries: document versus package ownership; package-local names and
  exported interfaces. Package bytes remain immutable during document edits.
- Execution: deterministic compilation and derivation; existing seeded
  sampling is unchanged. No new time axis or linguistic transition is added.
- Identity: package namespaces and document node IDs survive name resolution;
  qualification does not create a new lexical/construction identity.
- Persistence/API: canonical `.lang`, identity sidecars, package and ChangeSet
  locks, compiler/checker, app and CLI share resolution.
- Verification: `cargo test --workspace --locked --offline`; targeted parser, language,
  ChangeSet and persistence integration tests first.

## Contract

`export trait`, `export marker trait`, `export global trait`, `export sign`
and `export paradigm` retain visibility in the AST and canonical source.
Declarations without `export` remain usable inside their own package or
caller document. Only explicit exports may be referenced or overridden by
other packages/documents. `exports.tsv` supplies optional stable-ID metadata;
it cannot promote private declarations to public ones.

Qualified references use `Alias.Name`. Project `[packages.aliases]` maps an
alias to a selected package. Package names are default qualifiers when unique.
Package-internal references first resolve locally, then against dependencies;
caller references first resolve locally, then against selected exports.
An ambiguous bare reference is an error, not a load-order choice.
Package manifests may define their own `[aliases]` for the package itself or
its declared dependency closure. Project aliases and unrelated selected roots
cannot change those package-internal bindings.

Private dependencies of an exported declaration remain available to its
implementation, but cannot be named externally. Local qualified declarations
may override exported declarations in the compiled document; package bytes
are never mutated. Private targets cannot be overridden. Existing document
edits remain identity-addressed and must validate through the same resolver.
An override replaces the whole definition; it does not merge individual fields.
Package-owned IDs cannot be edited through document Primitive Edits. Insert a
qualified local override first, then edit its document-owned ID.

Public evaluation and query entry points resolve the same names. Sign and
Paradigm catalogs omit private package declarations and expose usable qualified
names for ambiguous exports. Returned tokens may contain private implementation
members; their package identity and category closure are retained. Raw effective
ASTs remain available for inspection and are not a confidentiality boundary.

## Persistence and compatibility

`exported` survives canonical source, source/identity reopen, ChangeSet insert,
`.exported = true|false` updates and reconstruction. Compiler semantics version
is bumped to invalidate compiled artifacts from the old visibility rules.

The package semantic digest includes source visibility and manifest aliases.
Explicit project aliases have a separate `ResolvedPackages::alias_digest()`;
the compile cache key includes it alongside package locks and export metadata.
ChangeSet preludes generated with packages write
`package_aliases = sha256:<digest>` when explicit aliases are present. Resolve
and replay require an exact match, including the difference between absent and
present aliases. No explicit aliases means no extra prelude line. Byte locks
alone cannot prove that a qualified reference still targets the same package.

Schema-1 export tables remain required; schema-2 tables are optional metadata
for these three kinds. The existing `.chg` function export/call contract is
unchanged. Shipped public definitions now have explicit `export`; their package
digests therefore change. Existing project/history locks are not silently
updated. The checked-in English reconstruction fixture was regenerated from
the migrated package and reviewed for only export flags and computed lock changes.

## Evidence map

| Requirement | Path | Counterexample |
|---|---|---|
| export visibility | parser → AST → printer → package loader | TSV cannot export private trait |
| dot qualification | references → resolver → compiler → runtime | two packages exporting Noun |
| encapsulation | scoped lowering → validation → applications | exported paradigm calling private sign succeeds; direct call fails |
| modifications | local overlay → ChangeSet validation/replay | private qualified override fails |
| retention | source/identity/lock round trip | qualification and export survive reopen |
| cache/replay | project alias digest → cache key and ChangeSet locks | unchanged package bytes with changed alias target |
| host queries | compiled scope → lexicon/Paradigm catalogs | same-name public members display qualified names; private entries are absent |

Verification results, executable tests and remaining boundaries are recorded in
[the dated audit](../verification/package-exports_2026-10-16.md).
For authoring syntax, start with [the executable English package](../../tutorials/package-exports/README.md).
