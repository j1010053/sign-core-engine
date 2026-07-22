# Grambank functional subset

This package pins 25 grammatical parameters to **Grambank v1.0**.  It does
not vendor language observations, counts, or probabilistic distributions.
`features.tsv` is a small mapping table from official parameter IDs to the
stdlib trait aliases used by this engine.

- Dataset release: <https://doi.org/10.5281/zenodo.7740140>
- Official feature browser: <https://grambank.clld.org/parameters>
- Versioned CLDF source: <https://github.com/grambank/grambank/tree/v1.0/cldf>
- Grambank data and site content license: CC BY 4.0

The three coding/knowledge states retain Grambank's open-world distinction: `0` is an
explicit negative observation, `1` is an explicit positive observation, and
`?` means that available documentation does not establish either result; it
is not a third grammatical behavior.
Merely omitting a trait from a sign is not interpreted as `0`.

The `.lang` traits are descriptive synchronic evidence.  They do not encode
diachronic causes, frequencies, entrenchment, or universal derivational
rules.  Attach value traits to a dedicated grammar-profile sign when storing
a language-level coding, or to a construction sign when its local behavior
is the evidence for that coding.
