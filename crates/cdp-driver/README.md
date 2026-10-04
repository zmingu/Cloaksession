# cdp-driver
Wraps chromiumoxide: safe CDP enable-refcount gate (all domains allowed — Chromix has no Runtime/Network restriction), no-op bootstrap (Chromix owns identity/context emulation in its SDK), and the 8 browser-drive tools (navigate/click/type/extract/screenshot/evaluate + behavioral injection). Connects by fetching `webSocketDebuggerUrl` from `/json/version` then `Browser::connect(ws)`.
