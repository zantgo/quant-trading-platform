-- CR-10: candle-only purge — remove every non-candle column and table.
--
-- The platform's model is OHLCV. Everything dropped below was read off a
-- venue stream that is not the candle:
--
--   * mark_price / index_price / mark_index_spread_pct
--       The Hyperliquid `/info metaAndAssetCtxs` REST poll and the Bitget /
--         Bybit WS ticker fields. They fed `mark_index_spread`, the HL
--         derivatives poller, and the cluster estimator's mid anchor.
--   * liquidity_* (7 columns)
--       Per-bar summaries of the venue's forced-close stream: long/short
--         notional, net, event count, cascade state and intensity, and the
--         raw `LiquidityFlow` JSON.
--   * cluster_* (5 columns)
--       Summaries of a `LiquidationClusterMatrix` that was estimated from
--         open interest under an ASSUMED leverage distribution
--         (`maintenance_margin_rate`, `[1,3,5,10,20,50,100]`). A model input
--         built from an assumption, not a measurement.
--
-- `liquidation_events` and `liquidation_real_buckets` are dropped outright.
-- The forced-close table's honest argument — "the venue published it" — does
-- not survive contact with the rest of the system: it existed to feed a chain
-- with no reader, and keeping it would keep the write path, the retention
-- prune, the DS mirror and two tables alive for nothing.
--
-- The venue still DECODES forced-close frames. Only everything downstream of
-- decode is gone.
--
-- `state_bands_json` REPLACES the heatmap slot: the same chart band geometry,
-- derived from candle structure alone (volume profile + loaded candles). It is
-- the one addition here, and it is a candle reading — which is the only kind
-- this table takes now.
--
-- ── Note on SQLite DROP COLUMN ──────────────────────────────────────────────
-- `ALTER TABLE ... DROP COLUMN` requires SQLite ≥ 3.35 and refuses to drop a
-- column that an index depends on. The partial index below is dropped first,
-- in that order, for exactly that reason. (The rebuild-the-table alternative
-- was rejected: it would rewrite every operator's existing 7-day snapshot
-- window for no behavioural gain.)

-- Every index whose expression names a dropped column, dropped first. SQLite
-- refuses a DROP COLUMN that an index still depends on, and it surfaces the
-- dependency at index-verification time rather than at parse time — so this
-- list has to be complete, not representative. (`idx_snapshots_mark_price` is
-- the subtle one: a partial index over `WHERE mark_price IS NOT NULL`.)
DROP INDEX IF EXISTS idx_snapshots_liquidity_cascade;
DROP INDEX IF EXISTS idx_snapshots_mark_price;

ALTER TABLE market_snapshots DROP COLUMN mark_price;
ALTER TABLE market_snapshots DROP COLUMN index_price;
ALTER TABLE market_snapshots DROP COLUMN mark_index_spread_pct;

ALTER TABLE market_snapshots DROP COLUMN liquidity_long_usd;
ALTER TABLE market_snapshots DROP COLUMN liquidity_short_usd;
ALTER TABLE market_snapshots DROP COLUMN liquidity_net_usd;
ALTER TABLE market_snapshots DROP COLUMN liquidity_events;
ALTER TABLE market_snapshots DROP COLUMN liquidity_cascade_state;
ALTER TABLE market_snapshots DROP COLUMN liquidity_cascade_intensity;
ALTER TABLE market_snapshots DROP COLUMN liquidity_json;

ALTER TABLE market_snapshots DROP COLUMN cluster_long_count;
ALTER TABLE market_snapshots DROP COLUMN cluster_short_count;
ALTER TABLE market_snapshots DROP COLUMN cluster_total_notional_usd;
ALTER TABLE market_snapshots DROP COLUMN cluster_estimation_confidence;
ALTER TABLE market_snapshots DROP COLUMN cluster_json;

ALTER TABLE market_snapshots ADD COLUMN state_bands_json TEXT;

DROP TABLE IF EXISTS liquidation_real_buckets;
DROP TABLE IF EXISTS liquidation_events;