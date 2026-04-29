# Protobuf sync — `ToggleFlashCustom`

The `Misc` field **`toggle_flash_custom`** (message `ToggleFlashCustom`, tag **39**) must stay identical in:

| Location | Path |
|----------|------|
| Web console (protobufjs) | `web-nodejs/protos/message.proto` |
| BetterDesk MGMT (prost) | `betterdesk-mgmt/src-tauri/protos/message.proto` |
| RustDesk / hbb_common | `rustdesk/libs/hbb_common/protos/message.proto` |

After editing `.proto` files:

1. **MGMT:** `cd betterdesk-mgmt/src-tauri && cargo build` — regenerates `src/proto/hbb.rs` (or merge the same changes if `hbb.rs` was edited by hand until build succeeds).
2. **RustDesk:** `cd rustdesk/libs/hbb_common && cargo build` — codegen writes to `OUT_DIR`; full `rustdesk` build pulls that in.

Do **not** change field numbers after release without a version negotiation story; peers with mismatched `Misc` oneofs may mis-decode.
