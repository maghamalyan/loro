---
"loro-crdt": patch
---

Rust undo and redo return application errors and roll back the failed batch, preserving document state and history stacks. Compound undo operations serialize against other document writers.
