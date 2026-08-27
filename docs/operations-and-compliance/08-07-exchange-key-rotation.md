# Exchange Key Rotation Procedure

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Operator procedure for rotating the `EXCHANGE_SECRET_KEY` (master encryption key for the `exchange_keys` SQLite table) and re-encrypting credentials without losing access to live exchange connections.

---

## 1. Background

The platform encrypts exchange credentials (api_key, api_secret, passphrase) at rest using AES-256-GCM with a master key derived from the `EXCHANGE_SECRET_KEY` environment variable. The encryption contract is in [06-02 §3.8](../integration-and-api/06-02-database-schema-spec.md). The master key is **never persisted**; it lives only in process memory for the duration of the daemon.

If `EXCHANGE_SECRET_KEY` is lost or rotated, all existing `exchange_keys` rows become unreadable. This document describes how to rotate without losing access.

---

## 2. Pre-rotation checklist

1. **Inventory existing keys.** `sqlite3 telemetry.db "SELECT key_id, exchange, created_at, last_rotated_at FROM exchange_keys;"` — record every `key_id`.
2. **Confirm operator UI access** — keys must be re-entered via `POST /api/keys` if rotation requires re-encryption from scratch.
3. **Schedule a maintenance window.** Rotation requires a daemon restart (Ops Phase 1+ will support hot-rotation; v6.4 requires restart).
4. **Backup `telemetry.db`.** `cp telemetry.db telemetry.db.backup-pre-rotation-$(date +%Y%m%d)`.

---

## 3. Rotation procedure

### 3.1 Generate a new master key

```bash
NEW_KEY=$(openssl rand -hex 32)   # 256 bits, hex-encoded
echo "$NEW_KEY" > /tmp/new_exchange_secret_key
chmod 600 /tmp/new_exchange_secret_key
```

### 3.2 Decrypt-and-re-encrypt script (v6.0: manual via API)

v6.4 does **not** ship an in-process rotation tool; the operator uses the documented `POST /api/keys` endpoint. The flow:

> **Warning.** Re-inserting keys before switching the master key does not rotate anything — rows remain encrypted under the old key.

1. **Record all credentials out-of-band.** For every `key_id` from the pre-rotation inventory (§2), copy the api_key/api_secret/passphrase from the exchange's UI or the operator's secret store. After step 3 the existing rows are unreadable — this record is the only copy.
2. **Stop the daemon:** `./manage.sh stop`.
3. **Start the daemon with the new master key:** `EXCHANGE_SECRET_KEY=$NEW_KEY ./execution-daemon`. The existing `exchange_keys` rows were encrypted under the old key and are now unreadable; the daemon treats the store as unconfigured.
4. **Re-insert every credential** via `POST /api/keys` with the same `exchange`; each insert encrypts under the new key. The new row replaces the old (Ops Phase 2 endpoint will support UPDATE; v6.4 requires DELETE + INSERT).
5. **Verify `ws::account` auth per key** — each key's private account stream must authenticate against the newly inserted credentials.
6. **Scrub the old key from every environment source** (shell environment, systemd units, `.env` files, secret managers) so it cannot be reused by accident.
7. Smoke test: confirm a trade tick arrives within 60 s.

### 3.3 Cleanup

```bash
unset NEW_KEY
rm -f /tmp/new_exchange_secret_key
```

Repeat the re-insertion (§3.2 step 4) for every `key_id` before decommissioning the old key.

---

## 4. Emergency rotation (suspected compromise)

If `EXCHANGE_SECRET_KEY` is suspected to have been exposed:

1. **Immediate:** stop the daemon.
2. **Rotate exchange-side credentials first** (regenerate api_key/api_secret on the exchange's UI).
3. Re-enter the new exchange credentials with the **new** master key (skip §3.2 step 1-2; start directly with the new key on an empty `exchange_keys` table).
4. **Do not reuse** the compromised master key.
5. Audit `telemetry.db` for unexpected `exchange_keys` activity during the exposure window.

---

## 5. Future work (open; target: next minor)

- `POST /api/keys/rotate` — in-process re-encryption under a new master key without a daemon restart.
- Hot key rotation via SIGHUP — daemon re-reads `EXCHANGE_SECRET_KEY` on signal.
- Encrypted-backup export — `GET /api/keys/backup` returns an encrypted blob keyed by a passphrase, suitable for off-machine storage.

These are tracked under `AUDIT-V6-077` (in-process exchange-key-rotation tool, Unscheduled) in `docs/CHANGELOG.md` — a tracked future tool, not an existing one; until it ships, the §3.2 manual flow remains the rotation path.

---

## 6. Cross-References

- Encryption contract: [06-02 §3.8](../integration-and-api/06-02-database-schema-spec.md)
- `/api/keys` endpoint: [06-01 §2.10](../integration-and-api/06-01-api-gateway-contract.md)
- User manual: [08-01](../operations-and-compliance/08-01-user-manual.md)
- Operator identity model: [06-01 §1](../integration-and-api/06-01-api-gateway-contract.md)