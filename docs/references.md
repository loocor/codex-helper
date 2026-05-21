# References

Codex Helper currently uses two public projects as references:

- BigPizzaV3/CodexPlusPlus: external launcher and CDP-based runtime injection mechanics
- b-nnett/codex-plusplus: tweak-oriented UI and local extension experience patterns

CodexMan at `/Volumes/External/GitHub/CodexMan` is the local reference checkout for studying dynamic injection. Codex Helper should borrow only the launch, CDP bridge, runtime injection, and selected tweak-context ideas.

Codex Helper should not inherit Provider, relay, ads, updater, installer, or manager systems from CodexMan. It should also avoid adding duplicate UI when Codex already has a native surface, such as restoring the existing Zed open target instead of adding a separate Zed button. The project keeps its own implementation small, local, and focused on pure tweak injection.
