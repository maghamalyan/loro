---
"loro-crdt": patch
---

Transform overlapping text attributes during undo so a later editor's formatting is preserved. Retain insertion ordering independently of attribute conflict priority. Test retained text trackers against rich-text values, including marks.
