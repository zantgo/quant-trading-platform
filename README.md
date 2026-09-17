# Quant Trading Platform

> **High-performance market telemetry monitor for Hyperliquid and Bitget, built in Rust.**

> **Single-operator local deployment.** The Trading Platform is built for a single operator and their team: one workspace, one operator identity (`local`), no per-route authentication, and no multi-client/SaaS model. Everything runs on your machine or a VM you control.

The **Trading Platform** processes high-resolution exchange telemetry and transforms raw data into real-time technical indicator visualizations, with optional paper-trading automation layered on top. It computes 50 technical indicators across 8 functional groups (Trend, Momentum, Volume, Volatility, Structure, Regime, Institutional, Derivatives Data) with 100 signal-kind declarations across 12 SignalKind types (Divergence, Crossover, Threshold, Breakout, BandTouch, ZeroLineCross, CompressionRelease, LevelTest, TrendFlip, VolumeClimax, StackChange, PatternForming), 8 of which support bull/bear divergence detection. All computation runs in Rust, streaming live data to a Svelte 5 dashboard via WebSocket.

> **Implementation status (v7).** All five engines are **implemented**: DIE (ingestion/reconstruction/persistence) and MME (52 indicators, 4-TF synthesis, decision layer) are complete; TAE is the v7 setup executor on the unified paper execution engine; PME is the informational portfolio mirror; PAE ships live analytics plus the recorded-decision backtest with full significance treatment. Paper trading is production-ready; live broker dispatch lands via `ExecutionBackend::LiveBroker`. See [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Quick Start Workflow

The system provides a unified helper script (`manage.sh`) at the root level to simplify building, running, and testing the entire workspace.

### 1. Common Workflow Commands

All key operations can be executed directly from the root directory:

```bash
# Make the manager script executable
chmod +x manage.sh

# 1. Install dependencies & build all components (Frontend + Backend)
./manage.sh build

# 2. Run the platform in the foreground (with live terminal logs)
./manage.sh run

# 3. Run the platform in the background (with silent logs writing to engine.log)
./manage.sh run-silent

# 4. Check status or stop the background execution
./manage.sh status
./manage.sh stop

# 5. Run all test suites (Rust unit/integrations + Svelte 5 Vitest)
./manage.sh test

# 6. Stop engine, clean builds, and permanently delete telemetry.db
./manage.sh destroy
```

Once running, navigate to http://127.0.0.1:3000 to access the dashboard.

## Workspace Structure

The platform is a 9-crate Cargo workspace + Svelte 5 frontend. The five logical engines (DIE, MME, TAE, PME, PAE) are mapped onto these physical crates:

- `crates/core-domain` — Stateless DTOs (`MarketSnapshot`, matrices), JSON-RPC, indicator value types.
- `crates/config-models` — All `*Config` structs + `load_config()` / `load_instances()` readers.
- `crates/market-analyzer` — 50 technical indicators, signals, multi-TF pipeline, decision support, liquidity math (MME).
- `crates/database-storage` — SQLite schema, migrations, WAL telemetry logger, queries (DIE persistence layer).
- `crates/network-adapters` — WS/REST clients (Hyperliquid, Bitget), NTP clock monitor, candle reconstruction, connection-quality tracker (DIE).
- `crates/portfolio-supervisor` — Instance lifecycle, position sizing, safety veto, session state, risk/commission math (PME + TAE).
- `crates/performance-analytics` — Dashboard stats compiler, strategy optimizer (PAE).
- `crates/api-gateway` — Axum HTTP router, WS broadcast server, HTTP handlers and request/response shapes.
- `crates/execution-daemon` — Headless CLI binary that wires everything together (`--web` mode).
- `ui` — Svelte 5 dashboard with interactive charting, real-time data, and market analysis tools.

## Documentation

The complete institutional documentation set lives under [`docs/`](docs/), organized into conceptual foundations, matrix data contracts, per-engine specifications, integration/API references, and UI/UX layouts.

| Document | Audience | Description |
|---|---|---|
| **[Global Architecture](docs/conceptual-foundations/01-02-global-architecture.md)** | Developers | Two-dimensional framework: 5 engines x sequenced analytical layers |
| **[Ontology](docs/conceptual-foundations/01-01-ontology.md)** | Developers | Formal ontology: engines, layers, matrices, 12 evaluation axes |
| **[Systemic Data Flow](docs/conceptual-foundations/01-03-systemic-data-flow.md)** | Developers | Chronological data-flow sequences across all engines |
| **[Timeframe Model](docs/conceptual-foundations/01-04-timeframe-model.md)** | Developers | Configurable 4-tier timeframe model (micro/fast/slow/macro) |
| **[Matrices](docs/matrices/02-00-matrix-field-ownership.md)** | Developers | Physical schemas and JSON contracts for all 11 matrices (MME + DIE) |
| **[Market Monitoring Engine](docs/engines/market-monitoring-engine/03-02-01-mme-overview-spec.md)** | Developers | 7 layer specs, indicator guide, signal guide, per-indicator specs |
| **[Trade Automation Engine](docs/engines/trade-automation-engine/03-03-01-tae-overview-spec.md)** | Developers | Policy + Execution layer specs, paper trading, execution policy spec |
| **[Portfolio Management Engine](docs/engines/portfolio-management-engine/03-04-01-pme-overview-spec.md)** | Developers | Position + Exposure + Capital + Portfolio layer specs |
| **[Performance Analytics Engine](docs/engines/performance-analytics-engine/03-05-01-pae-overview-spec.md)** | Developers | Trade + Strategy + Risk + Performance layer specs |
| **[Integration & API](docs/integration-and-api/06-01-api-gateway-contract.md)** | Developers | REST/WebSocket/JSON-RPC contracts and database schema |
| **[UI/UX](docs/ui-ux/07-01-ui-overview-spec.md)** | Frontend | Svelte 5 state management and dashboard layout specifications |
| **[AGENTS.md](AGENTS.md)** | Maintainers | Build instructions, runtime details, testing conventions |
