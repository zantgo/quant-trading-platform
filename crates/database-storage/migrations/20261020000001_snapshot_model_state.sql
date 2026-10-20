-- P10-G step 2.3: persist the model state alongside the legacy analysis matrix.
-- `analysis_json` stays for pre-dissolve recorded sessions until step 3 drops it.
ALTER TABLE market_snapshots ADD COLUMN model_state_json TEXT;
