# Test vectors

Copied ur-rs 0.5 (MIT) tables: `ur_rs_multipart_20.txt`, `ur_rs_choose_fragments.txt`, `wolf256_fragments.hex`. Public spec singles: `published_single.txt` (`ur:test/lsadaoaxjygonesw`, `ur:bytes/iehsjyhspmwfwfia`, Wolf/50 `ur:bytes/hdey…`). See repository-root `THIRD_PARTY.md`.

`published_from_refs.txt` is the **full** sorted-unique set of quoted `ur:…` literals from the pinned files (not vendored source). Regenerate:

```text
scripts/extract_published_urs.sh > crates/bcur/tests/vectors/published_from_refs.txt
```

Pins: URKit tag `15.1.0` (`c0a447560768e2552cf85a586dea8cfc26162891`) `Tests/URKitTests/URTests.swift` and `FountainCodesTests.swift`; bc-ur `4479fb81b2350ae8bafa042a5572b9c64c2c32ca` `test/test.cpp`. Those files do not quote the BCR `ur:test/lsadaoaxjygonesw` or `ur:bytes/iehsjyhspmwfwfia` singles; do not add them to the allowlist (weekly job is an exact-set diff). Weekly `.github/workflows/vectors-weekly.yml` is **drift detection**, not BCR-2020-005 compliance.
