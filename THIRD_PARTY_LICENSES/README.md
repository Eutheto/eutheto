# Third-party license texts

This directory is reserved for verbatim license texts and other license documents required by the exact third-party material distributed in eutheto artifacts.

At the Phase-00 empty baseline, there are no generated license-text files because no release dependency graph or third-party release material has been locked. Candidate dependencies named in the roadmap do not belong here unless and until an exact version is selected, reviewed under `THIRD_PARTY_NOTICES.md`, and included in an artifact. This state does not represent legal approval or a dependency audit.

This `README.md` is the hand-maintained directory policy. Once `xtask licenses generate` exists, every other file in this directory is generator-owned and must never be edited, renamed, or deleted manually. Correct the committed manifest, lockfile, license decision, provenance record, or generator and regenerate instead. Generated filenames must be deterministic and collision-resistant, and the generated notice inventory must refer to them unambiguously.

The generator must collect the license text from the selected artifact or its verified upstream source, preserve it verbatim, associate it with the exact component version or immutable material revision, and detect when packages with the same apparent license identifier carry different text, exceptions, or notices. Deduplication is permitted only for byte-identical texts when every inventory entry retains an exact reference. Missing or conflicting text is a release-blocking error; the generator must not synthesize terms from an SPDX identifier.

Generated license texts and `THIRD_PARTY_NOTICES.md` are authoritative inputs to release verification and packaging. Each assembled target must include all texts required by its exact contents, and generated drift, an orphaned inventory reference, an unreferenced shipped component, or disagreement with the release SBOM blocks release.
