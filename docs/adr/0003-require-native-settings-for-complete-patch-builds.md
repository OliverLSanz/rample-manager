---
status: superseded by 0007
---

# Require native settings for complete Patch builds

Build Patch is complete only when the Rample can load the intended audio layout
and native settings, including layer playback modes. Library and Kit management
may be developed earlier, but audio-folder export alone must not be represented as
a complete SD reconstruction. The current application does not read or write
`_save/*.rpl`; format mappings and round trips require verification against actual
Rample saves and hardware before a full native build can be promised. Database
persistence of app preferences is not a substitute for native configuration support.
