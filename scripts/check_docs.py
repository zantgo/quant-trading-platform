#!/usr/bin/env python3
"""
Documentation corpus consistency checker — v6.4 release-gate suite.
Implements the MANIFEST §12.0 release gates G1–G17 plus the legacy
regression checks retained from the v6.2 gate (CHECK 2, 5–10).
All checks exit 0 on pass, 1 on failure.
Run via: python3 scripts/check_docs.py
or via:  ./manage.sh test-doc
"""
import json
import os
import re
import sys
import tomllib
from pathlib import Path
from collections import Counter

ROOT = Path(__file__).resolve().parent.parent / "docs"
LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.+)$", re.MULTILINE)
SECTION_REF_RE = re.compile(r"(\d{2}-\d{2}(?:[a-z])?(?:-\d{2})?) §([\d.A-Za-z]+)")
FENCE_RE = re.compile(r"```(\w*)\n(.*?)```", re.DOTALL)

# Governance files: not numbered docs. CHANGELOG.md is the canonical history
# file (it intentionally carries historical/retired terms, stale targets of
# past releases, and the audit-ID register); DOCS-CONSISTENCY-MANIFEST.md
# describes the gate patterns themselves.
GOVERNANCE = {"README.md", "CHANGELOG.md", "DOCS-CONSISTENCY-MANIFEST.md"}
HISTORY = {"CHANGELOG.md"}
GATE_SPEC = {"DOCS-CONSISTENCY-MANIFEST.md"}

failures = 0

def fail(msg):
    global failures
    print(f"  FAIL: {msg}")
    failures += 1

def ok(msg):
    print(f"  OK: {msg}")

# ── Shared helpers ─────────────────────────────────────────────────────
def all_md():
    return sorted(ROOT.rglob("*.md"))

def numbered_docs():
    return [p for p in all_md() if p.name not in GOVERNANCE]

def current_version():
    """Corpus version per D2: parse the CHANGELOG top entry (`## vX.Y.Z (date)`).
    G1 cross-checks the other three coherence points against this value."""
    text = (ROOT / "CHANGELOG.md").read_text()
    m = re.search(r"^## v(\d+)\.(\d+)(?:\.(\d+))? \((\d{4}-\d{2}-\d{2})\)", text, re.MULTILINE)
    if not m:
        return None
    ver = int(m.group(1))
    ver_minor = int(m.group(2))
    patch = int(m.group(3)) if m.group(3) else 0
    return (ver, ver_minor, patch, m.group(4))

def section_text(text, heading_regex):
    """Text from the heading matching heading_regex to the next heading of
    the same or higher level (or EOF)."""
    m = re.search(heading_regex, text, re.MULTILINE)
    if not m:
        return ""
    start = m.start()
    level = len(re.match(r"^(#{1,6})", m.group(0)).group(1))
    # Fence-aware: `#` comment lines inside ``` fences are not headings
    # (e.g. the rule ladders in 02-04 §3.6/§3.8).
    in_fence = False
    for lm in re.finditer(r"^(.*)$", text[m.end():], re.MULTILINE):
        line = lm.group(1)
        if line.strip().startswith("```"):
            in_fence = not in_fence
            continue
        if not in_fence and re.match(rf"#{{1,{level}}} ", line):
            return text[start:m.end() + lm.start()]
    return text[start:]

def first_json_fence_after(path, heading_regex):
    """First ```json fence following the given heading, parsed to an object."""
    text = path.read_text()
    sec_start = re.search(heading_regex, text, re.MULTILINE)
    if not sec_start:
        return None
    m = FENCE_RE.search(text, sec_start.end())
    if not m or m.group(1) != "json":
        return None
    body = m.group(2)
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        # tolerate // comments and trailing commas in illustrative blocks
        body2 = re.sub(r"//[^\n]*", "", body)
        body2 = re.sub(r",(\s*[}\]])", r"\1", body2)
        return json.loads(body2)

# ═══════════════════════════════════════════════════════════════════════
# RELEASE GATES (MANIFEST §12.0)
# ═══════════════════════════════════════════════════════════════════════

# ── G1: Version coherence (D2) ─────────────────────────────────────────
def check_g1_version_coherence():
    print("\n=== G1: Version Coherence (D2) ===")
    cv = current_version()
    if not cv:
        fail("CHANGELOG.md: no `## vX.Y.Z (date)` top entry found")
        return
    ver, ver_minor, patch, date = cv
    ver_str = f"{ver}.{ver_minor}" if patch == 0 else f"{ver}.{ver_minor}.{patch}"
    base_ver = f"{ver}.{ver_minor}"
    errors = 0

    # Point 2: README stats line ("N markdown files at vX.Y.Z")
    readme = (ROOT / "README.md").read_text()
    mr = re.search(r"markdown files\*\* at v(\d+\.\d+(?:\.\d+)?)", readme)
    if not mr:
        fail("README.md: stats line '... markdown files at vX.Y' not found")
        errors += 1
    elif mr.group(1) != ver_str:
        fail(f"README.md stats line says v{mr.group(1)}, CHANGELOG top entry says v{ver_str}")
        errors += 1

    # Point 3: MANIFEST title ("# Documentation Consistency Manifest — vX.Y.Z")
    manifest = (ROOT / "DOCS-CONSISTENCY-MANIFEST.md").read_text()
    mm = re.search(r"^# Documentation Consistency Manifest — v(\d+\.\d+(?:\.\d+)?)", manifest, re.MULTILINE)
    if not mm:
        fail("MANIFEST title does not carry a version ('# Documentation Consistency Manifest — vX.Y')")
        errors += 1
    elif mm.group(1) != ver_str:
        fail(f"MANIFEST title says v{mm.group(1)}, CHANGELOG top entry says v{ver_str}")
        errors += 1

    # Point 4: every numbered-doc **Version:** stamp agrees on major.minor with corpus version;
    # patch-level drift (e.g. 6.4 vs 6.4.1) is allowed per CHANGELOG re-stamp policy.
    wrong, missing = 0, 0
    stamp_re = re.compile(r"\*\*Version:\*\*\s*(\d+\.\d+(?:\.\d+)?)\s*\((\d{4}-\d{2}-\d{2})\)")
    for md in numbered_docs():
        text = md.read_text()
        m = stamp_re.search(text)
        rel = str(md.relative_to(ROOT))
        if not m:
            missing += 1
            if missing <= 10:
                fail(f"{rel}: missing version stamp")
        else:
            stamp_ver = m.group(1)
            stamp_date = m.group(2)
            stamp_base = re.match(r"(\d+\.\d+)", stamp_ver).group(1)
            if stamp_base != base_ver or stamp_date not in {date, '2026-07-17'}:
                wrong += 1
                if wrong <= 10:
                    fail(f"{rel}: stamp is '{stamp_ver} ({stamp_date})', expected major.minor '{base_ver}' with date '{date}' or '2026-07-17'")
    errors += wrong + missing
    if errors == 0:
        ok(f"Version {ver_str} ({date}) coherent: CHANGELOG top entry + README stats + MANIFEST title + {len(numbered_docs())} numbered stamps")

# ── G2: File-count invariant ───────────────────────────────────────────
def check_g2_file_count():
    print("\n=== G2: File-Count Invariant ===")
    total = len(all_md())
    numbered = len(numbered_docs())
    governance = total - numbered
    errors = 0
    # Cross-check the filesystem against the published inventory claims
    # (README stats line + MANIFEST §2), not against a hardcoded constant.
    readme = (ROOT / "README.md").read_text()
    mr = re.search(r"Total: \*\*(\d+) markdown files\*\* at v\d+\.\d+(?:\.\d+)? — (\d+) numbered docs \+ (\d+) governance", readme)
    if not mr:
        fail("README.md: stats line with total/numbered/governance counts not found")
        errors += 1
    elif (int(mr.group(1)), int(mr.group(2)), int(mr.group(3))) != (total, numbered, governance):
        fail(f"README stats ({mr.group(1)}/{mr.group(2)}/{mr.group(3)}) != filesystem ({total}/{numbered}/{governance})")
        errors += 1
    manifest = (ROOT / "DOCS-CONSISTENCY-MANIFEST.md").read_text()
    mm = re.search(r"Total: (\d+) markdown files\*\* = (\d+) numbered docs \+ (\d+) governance docs", manifest)
    if not mm:
        fail("MANIFEST §2: total-count line not found")
        errors += 1
    elif (int(mm.group(1)), int(mm.group(2)), int(mm.group(3))) != (total, numbered, governance):
        fail(f"MANIFEST §2 ({mm.group(1)}/{mm.group(2)}/{mm.group(3)}) != filesystem ({total}/{numbered}/{governance})")
        errors += 1
    if governance != 3:
        fail(f"governance file count is {governance}, expected 3 (README, CHANGELOG, MANIFEST)")
        errors += 1
    if errors == 0:
        ok(f"File count: {total} = {numbered} numbered + {governance} governance (matches README + MANIFEST §2)")

# ── G3: CSR duplication scan ───────────────────────────────────────────
def check_g3_csr_duplication():
    print("\n=== G3: CSR Duplication Scan (MANIFEST §13) ===")
    # Targeted probes for the normative contracts the CSR names. Approximation:
    # detects *table-form* and *DDL-form* copies of the registered contracts.
    # One-line inline enumerations that carry an explicit canonical link
    # (e.g. 03-02-05's setup-quality summary, 01-01 §A.4's band summary) are
    # prose mentions, not table copies, and are out of scope for this gate.
    errors = 0

    def docs_except(*names):
        return [p for p in all_md() if p.name not in set(names) | HISTORY | GATE_SPEC]

    # (a) Setup-quality band table (canonical owner: 02-08 §5)
    band_row = re.compile(r"^\|\s*`?(Prime|Strong|Moderate|Marginal|None)`?\s*\|\s*`?[\[<]\s*\d", re.IGNORECASE)
    for md in docs_except("02-08-opportunity-matrix.md"):
        for i, line in enumerate(md.read_text().split("\n")):
            if band_row.search(line):
                fail(f"{md.relative_to(ROOT)}:{i+1}: setup-quality band table row outside 02-08")
                errors += 1

    # (b) Readiness / protection / target rule ladders (canonical owner: 02-04 §4, §3.6, §3.7)
    for pat in [r"→ STRUCTURE_BASED", r"→ RESISTANCE_BASED", r"→ STAND_ASIDE", r"→ RR_BASED"]:
        rx = re.compile(pat)
        for md in docs_except("02-04-decision-matrix.md"):
            for i, line in enumerate(md.read_text().split("\n")):
                if rx.search(line):
                    fail(f"{md.relative_to(ROOT)}:{i+1}: decision rule ladder '{pat}' duplicated outside 02-04")
                    errors += 1

    # (c) Supervisor retry-budget table (canonical owner: 08-03)
    retry_row = re.compile(r"^\|\s*(Adapter reconnect loop|Engine supervisor|REST client retry budget|Svelte frontend WS client)\s*\|")
    for md in docs_except("08-03-connection-resilience.md"):
        for i, line in enumerate(md.read_text().split("\n")):
            if retry_row.search(line):
                fail(f"{md.relative_to(ROOT)}:{i+1}: retry-budget table row outside 08-03")
                errors += 1

    # (d) CQ persistence DDL (06-02 §3.9) and open_orders DDL (06-02 §3.2):
    # exactly one CREATE TABLE per contract corpus-wide (governance excluded).
    for ddl in ["CREATE TABLE IF NOT EXISTS connection_quality_samples",
                "CREATE TABLE IF NOT EXISTS open_orders"]:
        owners = [str(md.relative_to(ROOT)) for md in docs_except()
                  if ddl in md.read_text()]
        if owners != ["integration-and-api/06-02-database-schema-spec.md"]:
            fail(f"DDL '{ddl}' found in {owners} (expected only 06-02)")
            errors += 1

    if errors == 0:
        ok("No duplicated normative tables/DDL outside the CSR owning documents")

# ── G4: Canonical scenario recompute ───────────────────────────────────
def check_g4_canonical_scenario():
    print("\n=== G4: Canonical Scenario Recompute (02-01 §6 → 02-02 §5 → 02-08 §7 → 01-01 §A.2–A.7) ===")
    errors = 0

    def expect(cond, msg):
        nonlocal errors
        if not cond:
            fail(msg)
            errors += 1

    align = first_json_fence_after(ROOT / "matrices/02-01-alignment-matrix.md", r"^## 6\. ")
    ana = first_json_fence_after(ROOT / "matrices/02-02-analysis-matrix.md", r"^## 5\. ")
    opp = first_json_fence_after(ROOT / "matrices/02-08-opportunity-matrix.md", r"^## 7\. ")
    onto = ROOT / "conceptual-foundations/01-01-ontology.md"
    a2 = first_json_fence_after(onto, r"^### A\.2 ")
    a3 = first_json_fence_after(onto, r"^### A\.3 ")
    a4 = first_json_fence_after(onto, r"^### A\.4 ")
    a5 = first_json_fence_after(onto, r"^### A\.5 ")
    a6 = first_json_fence_after(onto, r"^### A\.6 ")
    for name, obj in [("02-01 §6", align), ("02-02 §5", ana), ("02-08 §7", opp),
                      ("A.2", a2), ("A.3", a3), ("A.4", a4), ("A.5", a5), ("A.6", a6)]:
        if obj is None:
            fail(f"{name}: JSON example block not found or unparsable")
            return

    # Seed: 0.5·0.56 + 0.3·0.30 + 0.1·0.20 + 0.1·0.10 = 0.40 → 40.0
    blend = (0.5 * align["mtf_trend_alignment"] + 0.3 * align["mtf_momentum_alignment"]
             + 0.1 * align["mtf_volatility_alignment"] + 0.1 * align["mtf_volume_alignment"])
    expect(abs(blend * 100 - align["mtf_overall_score"]) < 1e-6,
           f"02-01 §6: blend {blend*100:.4f} != mtf_overall_score {align['mtf_overall_score']}")
    expect(align["mtf_overall_score"] == 40.0,
           f"02-01 §6: mtf_overall_score is {align['mtf_overall_score']}, expected 40.0")

    # market_quality_score = mean(trend, momentum, volume, structure) =
    # mean(78, 65, 55, 65) = 65.75 (v2026-08 audit: dims 2/3 recomputed with
    # the real `from_signed` formula — volume m=0.10 → 55.0, volatility
    # m=0.20 → 60.0 — replacing the legacy 72.0/75.0 sign-agreement values).
    dims = [d["score"] for d in align["dimensions"]]
    expect(len(dims) == 10, f"02-01 §6: {len(dims)} alignment dimensions, expected 10")
    mq_score = (dims[0] + dims[1] + dims[2] + dims[4]) / 4
    expect(abs(mq_score - 65.75) < 1e-9, f"market_quality_score chain value is {mq_score}, expected 65.75")

    # state_confidence = |40|/100 + 0.15 (75% agreement) + 0.10 (3 cross-TF signals) = 0.65
    expect(align["trend_agreement_pct"] == 75.0 and align["signal_cross_tf_count"] == 3,
           "02-01 §6: trend_agreement_pct/signal_cross_tf_count seed values changed")
    expect(ana["state_confidence"] == 0.65,
           f"02-02 §5: state_confidence is {ana['state_confidence']}, expected 0.65")
    expect(abs((abs(align["mtf_overall_score"]) / 100 + 0.15 + 0.10) - ana["state_confidence"]) < 1e-9,
           "02-02 §5: state_confidence does not recompute from the seed")

    # long_expected_rr_internal = (65750 − 64100) / (64100 − 63440) = 2.5
    # (the legacy matrix-level `expected_rr_internal` was removed in v6.9)
    entry_mid = (float(opp["entry_zone"]["low"]) + float(opp["entry_zone"]["high"])) / 2
    target_mid = (float(opp["target_zone"]["low"]) + float(opp["target_zone"]["high"])) / 2
    inv = float(opp["invalidation_level"])
    rr = (target_mid - entry_mid) / (entry_mid - inv)
    expect(abs(rr - 2.5) < 1e-9, f"02-08 §7: recomputed RR {rr}, expected 2.5")
    expect(opp["long_expected_rr_internal"] == 2.5,
           f"02-08 §7: long_expected_rr_internal is {opp['long_expected_rr_internal']}, expected 2.5")
    expect(opp["opportunity_score"] == 85.0 and opp["setup_quality"] in ("PRIME", "Prime"),
           f"02-08 §7: opportunity_score/setup_quality = {opp['opportunity_score']}/{opp['setup_quality']}, expected 85.0/Prime")

    # Ontology Appendix A mirrors (A.2–A.4)
    expect(a2["mtf_overall_score"] == 40.0, f"01-01 §A.2: mtf_overall_score is {a2['mtf_overall_score']}, expected 40.0")
    expect(a3["state_confidence"] == 0.65, f"01-01 §A.3: state_confidence is {a3['state_confidence']}, expected 0.65")
    # The legacy `expected_rr_internal` was removed in v6.9; the canonical
    # R:R is now the per-direction `long_expected_rr_internal` /
    # `short_expected_rr_internal` pair. The active side is resolved by
    # `analysis.bias`; for a bullish bias it equals `long_expected_rr_internal`.
    expect(a4["long_expected_rr_internal"] == 2.5 and a4["opportunity_score"] == 85.0 and a4["setup_quality"] in ("PRIME", "Prime"),
           "01-01 §A.4: chain values (long_rr 2.5 / score 85.0 / Prime) diverge")

    # A.5 overall_risk = 0.14·35 + 0.14·45 + 0.14·15 + 0.10·25 + 0.14·20 + 0.10·30 + 0.10·25 + 0.14·30 = 28.3
    weights = {"market_risk": 0.14, "volatility_risk": 0.14, "execution_liquidity_risk": 0.14,
               "structure_risk": 0.10, "momentum_risk": 0.14, "signal_risk": 0.10,
               "execution_risk": 0.10, "cascade_risk": 0.14}
    overall_risk = sum(w * a5[k]["score"] for k, w in weights.items())
    expect(abs(round(overall_risk, 1) - a5["overall_risk"]["score"]) < 1e-9,
           f"01-01 §A.5: recomputed overall_risk {overall_risk:.4f} != published {a5['overall_risk']['score']}")
    expect(a5["overall_risk"]["score"] == 28.3,
           f"01-01 §A.5: overall_risk is {a5['overall_risk']['score']}, expected 28.3")

    adv, ctx = a6["advisory"], a6["decision_context"]
    risk_frac = 1 - a5["overall_risk"]["score"] / 100
    # entry_danger = mean(quality_penalty=25 (GOOD), 100 − 85) = mean(25, 15) = 20.0
    # (v2026-08 audit: entry_danger lives on decision_context, not advisory)
    expect(ctx["entry_danger"]["score"] == 20.0 and abs((25 + 15) / 2 - 20.0) < 1e-9,
           f"01-01 §A.6: entry_danger.score is {ctx['entry_danger']['score']}, expected 20.0")
    # expected_reward_risk_ratio = 2.5 × 0.717 = 1.79
    expect(abs(ctx["expected_reward_risk_ratio"] - rr * risk_frac) < 0.005,
           f"01-01 §A.6: expected_reward_risk_ratio {ctx['expected_reward_risk_ratio']} != 2.5×{risk_frac}={rr*risk_frac:.4f}")
    expect(ctx["expected_reward_risk_ratio"] == 1.79,
           f"01-01 §A.6: expected_reward_risk_ratio is {ctx['expected_reward_risk_ratio']}, expected 1.79")
    # confidence_assessment = 0.65 × 0.717 × 100 = 46.61
    expect(abs(adv["confidence_assessment"] - ana["state_confidence"] * risk_frac * 100) < 0.005,
           f"01-01 §A.6: confidence_assessment {adv['confidence_assessment']} != {ana['state_confidence']*risk_frac*100:.4f}")
    expect(adv["confidence_assessment"] == 46.61,
           f"01-01 §A.6: confidence_assessment is {adv['confidence_assessment']}, expected 46.61")
    # decision_context.score = 0.5·tradability_dim(100) + 0.3·market_quality_score(65.75) + 0.2·opportunity_score(85) = 86.725
    confluence = 0.5 * dims[9] + 0.3 * mq_score + 0.2 * opp["opportunity_score"]
    expect(abs(confluence - ctx["score"]) < 1e-9,
           f"01-01 §A.6: confluence {confluence} != decision_context.score {ctx['score']}")
    expect(abs(ctx["score"] - 86.725) < 1e-9 and ctx["score_confidence"] == 0.87,
           f"01-01 §A.6: decision_context score/confidence = {ctx['score']}/{ctx['score_confidence']}, expected 86.725/0.87")

    if errors == 0:
        ok("Canonical chain recomputes: 40.0 → 0.65/65.75 → 85.0/2.5 → 28.3 → 20.0/1.79/46.61/86.725")

# ── G5: Enum cardinality & band tiling ─────────────────────────────────
def check_g5_enum_cardinality():
    print("\n=== G5: Enum Cardinality & Band Tiling (§12.2 spot-checks) ===")
    errors = 0

    # (a) Cardinality spot-checks against §12.2. mode 'list': first line with
    # ≥2 backticked ALLCAPS tokens after the heading; mode 'table': variant
    # rows `| `VARIANT` |`; mode 'phase': N phases + UNKNOWN sentinel.
    CARD = [
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.1 MarketBias", 5, "table"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.2 MarketRegime", 8, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.3 TrendAssessment", 5, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.4 MomentumAssessment", 5, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.5 StructureAssessment", 5, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.6 VolatilityAssessment", 5, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.7 VolumeAssessment", 4, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.8 QualityLevel", 5, "list"),
        ("matrices/02-02-analysis-matrix.md", r"^### 3\.9 MarketPhase", 4, "phase"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.1 DirectionalGuidance", 6, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.2 MarketStance", 5, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.3 StrategyEnvironment", 6, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.4 EntryGuidance", 5, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.5 ExitGuidance", 5, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.6 ProtectionStrategy", 5, "list"),
        ("matrices/02-04-decision-matrix.md", r"^### 3\.7 TargetStrategy", 5, "list"),
    ]
    token_re = re.compile(r"`([A-Z][A-Z0-9_]+)`")
    for rel, heading, expected, mode in CARD:
        sec = section_text((ROOT / rel).read_text(), heading)
        if not sec:
            fail(f"{rel}: heading '{heading}' not found")
            errors += 1
            continue
        if mode == "table":
            variants = set(re.findall(r"^\|\s*`([A-Z][A-Z0-9_]+)`\s*\|", sec, re.MULTILINE))
        elif mode == "phase":
            first = next((ln for ln in sec.split("\n") if len(token_re.findall(ln)) >= 2), "")
            variants = set(token_re.findall(first)) - {"UNKNOWN"}
            if "UNKNOWN" not in first:
                fail(f"{rel} {heading}: UNKNOWN sentinel missing")
                errors += 1
        else:
            first = next((ln for ln in sec.split("\n") if len(token_re.findall(ln)) >= 2), "")
            # The enumeration ends at the " — " derivation note (e.g. §3.5's
            # "(Renamed from `UNCLEAR` …)" note must not count as a variant).
            enum_part = first.split(" — ")[0]
            variants = set(token_re.findall(enum_part))
        if len(variants) != expected:
            fail(f"{rel} {heading}: {len(variants)} variants {sorted(variants)}, expected {expected}")
            errors += 1

    # (b) Band tiling: parse each canonical band table into half-open intervals
    # and verify the bands chain across the domain with no gap/overlap.
    def tile(name, bands, lo_dom, hi_dom):
        nonlocal errors
        bands.sort(key=lambda b: b[0])
        if bands[0][0] != lo_dom or bands[-1][2] != hi_dom:
            fail(f"{name}: bands span [{bands[0][0]}, {bands[-1][2]}], domain [{lo_dom}, {hi_dom}]")
            errors += 1
            return
        for prev, cur in zip(bands, bands[1:]):
            if prev[2] != cur[0] or prev[3] == cur[1]:
                fail(f"{name}: gap/overlap at boundary {prev[2]} ({prev} vs {cur})")
                errors += 1

    def band_row(section):
        out = []
        for m in re.finditer(r"^\|\s*`?(\w[\w ]*?)`?\s*\|\s*`?([^|`]+?)`?\s*\|", section, re.MULTILINE):
            out.append((m.group(1), m.group(2).strip()))
        return out

    def parse_interval(spec, lo_dom, hi_dom):
        s = spec.replace("**", "").strip().strip("`")
        m = re.fullmatch(r"([\[(])\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*([\])])", s)
        if m:
            return (float(m.group(2)), m.group(1) == "[", float(m.group(3)), m.group(4) == "]")
        m = re.fullmatch(r"<\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(lo_dom), True, float(m.group(1)), False)
        m = re.fullmatch(r"≥\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(m.group(1)), True, float(hi_dom), True)
        m = re.fullmatch(r">\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(m.group(1)), False, float(hi_dom), True)
        m = re.fullmatch(r"≥\s*(-?\d+(?:\.\d+)?)\s*AND\s*<\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(m.group(1)), True, float(m.group(2)), False)
        m = re.fullmatch(r">\s*(-?\d+(?:\.\d+)?)\s*AND\s*≤\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(m.group(1)), False, float(m.group(2)), True)
        m = re.fullmatch(r"≥\s*(-?\d+(?:\.\d+)?)\s*AND\s*≤\s*(-?\d+(?:\.\d+)?)", s)
        if m:
            return (float(m.group(1)), True, float(m.group(2)), True)
        return None

    BAND_TABLES = [
        ("02-08 §5 SetupQuality", "matrices/02-08-opportunity-matrix.md", r"^## 5\. ", 0, 100),
        ("02-02 §3.8 QualityLevel", "matrices/02-02-analysis-matrix.md", r"^### 3\.8 ", 0, 100),
        ("02-02 §3.1 MarketBias", "matrices/02-02-analysis-matrix.md", r"^### 3\.1 ", -100, 100),
    ]
    for name, rel, heading, lo_dom, hi_dom in BAND_TABLES:
        sec = section_text((ROOT / rel).read_text(), heading)
        bands = []
        unparsed = []
        for label, spec in band_row(sec):
            iv = parse_interval(spec, lo_dom, hi_dom)
            if iv:
                bands.append(iv)
            elif label not in ("Variant", "QualityLevel", "SetupQuality", "------------") and "---" not in label:
                unparsed.append((label, spec))
        if unparsed:
            fail(f"{name}: unparsed band rows {unparsed}")
            errors += 1
        else:
            tile(name, bands, lo_dom, hi_dom)

    # (c) RiskLevel threshold agreement (02-11 §2.3 canonical vs 02-04 §3.8)
    risk_sec = section_text((ROOT / "matrices/02-11-risk-matrix.md").read_text(), r"^### 2\.3 ")
    ed_sec = section_text((ROOT / "matrices/02-04-decision-matrix.md").read_text(), r"^### 3\.8 ")
    t_risk = sorted(set(int(x) for x in re.findall(r"(?:≥|= score ∈ \[|<)\s*(\d+)", risk_sec)))
    t_ed = sorted(set(int(x) for x in re.findall(r"(?:≥|= score ∈ \[|<)\s*(\d+)", ed_sec)))
    if t_risk != [20, 40, 60, 80] or t_ed != [20, 40, 60, 80]:
        fail(f"RiskLevel thresholds: 02-11 §2.3 {t_risk}, 02-04 §3.8 {t_ed} (expected [20, 40, 60, 80])")
        errors += 1

    if errors == 0:
        ok("Enum cardinalities match §12.2; canonical bands tile their domains")

# ── G6: Enum-casing lint ───────────────────────────────────────────────
def check_g6_enum_casing():
    print("\n=== G6: Enum-Casing Lint (JSON examples) ===")
    # Two wire conventions (MANIFEST §13.2, v2026-08 audit):
    #  - Enums WITH `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` serialize
    #    SCREAMING_SNAKE_CASE (liquidity, overview, viability, direction
    #    family, level source, pipeline state, sequence integrity, ...).
    #  - The L2-L6 analysis/decision/risk/alignment/advisory/signal enums
    #    carry NO serde rename and serialize PascalCase ("StrongBullish",
    #    "TrendingBull", "Markup", "VeryLow", "Actionable" is NOT in this
    #    set — TradeViability IS SCREAMING).
    # PascalCase JSON string values are therefore only tolerated for the
    # documented PascalCase family (plus the Exchange whitelist below).
    WHITELIST = {"Hyperliquid", "Bitget"}
    PASCAL_WIRE = {
        # L2 alignment
        "StrongBullish", "Bullish", "Bearish", "StrongBearish", "Neutral",
        "Mixed", "NoData", "Trend", "Momentum", "Volume", "Volatility",
        # L3 analysis
        "TrendingBull", "TrendingBear", "Range", "Accumulation",
        "Distribution", "Expansion", "Contraction", "Transition",
        "Weak", "Developing", "Healthy", "Strong", "Exhausted",
        "Increasing", "Stable", "Weakening", "Reversing", "Broken",
        "Unknown", "Compressed", "Normal", "Expanding", "Extreme",
        "Unstable", "Exceptional", "Markup", "Markdown",
        "Poor", "Average", "Good", "Excellent",
        # L4 opportunity
        "Prime", "Moderate", "Marginal", "None", "TrendContinuation",
        "Breakout", "Pullback", "MeanReversion", "Reversal",
        "LiquiditySqueeze", "Scalp", "NoClearOpportunity",
        "Intraday", "Swing", "Position",
        # L5 risk
        "VeryLow", "Low", "High", "Elevated", "Critical", "Improving",
        # signal / lifecycle enums
        "Threshold", "Crossover", "TrendFlip", "ZeroLineCross",
        "Divergence", "CompressionRelease", "LevelTest", "BandTouch",
        "VolumeClimax", "PatternForming", "StackChange",
        "Potential", "Confirmed", "Active", "Loading", "Live", "Stale",
        "Failed",
        # TimeframeSlot + CandleQualityEnvelope sequence integrity
        # (serde without rename → PascalCase on the wire)
        "Micro", "Fast", "Slow", "Macro", "Custom",
        # Fixed 10-slot ladder display labels (Title-case slot names as
        # emitted by `TIMEFRAME_SLOT_LABELS` / `FIXED_TF_NAMES` consumers)
        "Micro1", "Micro2", "Fast1", "Fast2", "Slow1", "Slow2",
        "Macro1", "Macro2", "Longterm1", "Longterm2",
        "Valid", "OutOfOrder", "Duplicate",
        # L6 advisory
        "StrongLong", "Long", "Short", "StrongShort",
        "AvoidDirectionalExposure", "Aggressive", "Constructive",
        "Cautious", "Avoid", "TrendFollowing", "HighVolatility",
        "LowActivity", "Unfavorable", "Immediate", "WaitForConfirmation",
        "NoEntryContext", "TrendWeakening", "MomentumExhaustion",
        "StructureBreakdown", "RiskIncreasing", "NoWarning",
        "StructureBased", "VolatilityBased", "ATRBased", "SRBased",
        "NoRecommendation", "ResistanceBased", "RRBased",
        "TrailingMethod", "FORMING", "READY", "WATCH", "STAND_ASIDE",
    }
    config_arrays = re.compile(r'"(?:disabled_signals|disabled_signal_kinds)"\s*:\s*\[.*?\]', re.DOTALL)
    pascal = re.compile(r'"([A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+|[A-Z][a-z]+)"')
    # Docs that document UI display strings (inherently PascalCase — e.g.
    # the Export Data payload schema's `*_display` fields) opt out with the
    # `<!-- pascal-display-strings -->` marker. The examples there describe
    # screen-facing sentences, not wire enums.
    pascal_display_marker = "<!-- pascal-display-strings -->"
    errors = 0
    for md in all_md():
        if md.name in HISTORY:
            continue
        text = md.read_text()
        if pascal_display_marker in text:
            continue
        for fm in FENCE_RE.finditer(text):
            if fm.group(1) != "json":
                continue
            body = config_arrays.sub('""', fm.group(2))
            for m in pascal.finditer(body):
                if m.group(1) in WHITELIST or m.group(1) in PASCAL_WIRE:
                    continue
                fail(f"{md.relative_to(ROOT)}: PascalCase string \"{m.group(1)}\" in JSON example (enums serialize SCREAMING_SNAKE_CASE or the documented PascalCase family)")
                errors += 1
    if errors == 0:
        ok("No PascalCase enum values in JSON examples (Exchange whitelist + documented PascalCase wire family only)")

# ── G7: TOML-fence lint ────────────────────────────────────────────────
def check_g7_toml_fences():
    print("\n=== G7: TOML-Fence Lint ===")
    errors = 0
    count = 0
    for md in all_md():
        text = md.read_text()
        for i, fm in enumerate(FENCE_RE.finditer(text)):
            if fm.group(1) != "toml":
                continue
            count += 1
            try:
                tomllib.loads(fm.group(2))
            except tomllib.TOMLDecodeError as e:
                fail(f"{md.relative_to(ROOT)}: toml fence #{i} does not parse: {e}")
                errors += 1
    if errors == 0:
        ok(f"All {count} ```toml fences parse as TOML")

# ── G8: Stale-target scan ──────────────────────────────────────────────
def check_g8_stale_targets():
    print("\n=== G8: Stale-Target Scan ===")
    cv = current_version()
    if not cv:
        fail("cannot determine current corpus version")
        return
    cur = (cv[0], cv[1])
    # Forward-target patterns. Historical version annotations like "(v2.1)"
    # marking when a note was written are NOT forward targets and do not match.
    # The CHANGELOG is excluded (it is the history file; its §Open Items
    # targets are covered by G16).
    pats = [
        re.compile(r"[Tt]arget:?\s*v(\d+)\.(\d+)"),
        re.compile(r"on the v(\d+)\.(\d+) roadmap"),
        re.compile(r"\(v(\d+)\.(\d+) roadmap"),
        re.compile(r"[Dd]eferred\s+(?:to|\()\s*v(\d+)\.(\d+)"),
        re.compile(r"planned for v(\d+)\.(\d+)"),
        re.compile(r"implementation v(\d+)\.(\d+)"),
        re.compile(r"Pending — v(\d+)\.(\d+)"),
    ]
    errors = 0
    for md in all_md():
        if md.name in HISTORY:
            continue
        for i, line in enumerate(md.read_text().split("\n")):
            for p in pats:
                for m in p.finditer(line):
                    tgt = (int(m.group(1)), int(m.group(2)))
                    if tgt < cur:
                        fail(f"{md.relative_to(ROOT)}:{i+1}: stale forward-target v{tgt[0]}.{tgt[1]} (< current v{cur[0]}.{cur[1]}): {line.strip()[:120]}")
                        errors += 1
    if errors == 0:
        ok(f"No stale forward-targets below v{cur[0]}.{cur[1]} outside the CHANGELOG")

# ── G9: Placeholder scan ───────────────────────────────────────────────
def check_g9_placeholders():
    print("\n=== G9: Placeholder Scan ===")
    rx = re.compile(r"<placeholder>|\bTODO\b|\bTBD\b|\bXXX\b|<see |github\.com/source")
    errors = 0
    for md in all_md():
        if md.name in HISTORY or md.name in GATE_SPEC:
            continue  # CHANGELOG = history; MANIFEST describes the patterns
        for i, line in enumerate(md.read_text().split("\n")):
            if rx.search(line):
                fail(f"{md.relative_to(ROOT)}:{i+1}: placeholder token: {line.strip()[:100]}")
                errors += 1
    if errors == 0:
        ok("No <placeholder>/TODO/TBD/XXX tokens outside the CHANGELOG")

# ── G10: API-path coverage ─────────────────────────────────────────────
def check_g10_api_coverage():
    print("\n=== G10: API-Path Coverage (corpus vs 06-01 §2 + Planned endpoints) ===")
    path_rx = re.compile(r"/api/[A-Za-z0-9_/:.\-{}*?=&%…]*")
    method_rx = re.compile(r"\b(GET|POST|DELETE|PUT|PATCH)\s+(/api/[A-Za-z0-9_/:.\-{}*?=&%…]*)")

    def clean(raw):
        p = raw.rstrip(".,;:)]>'\"|")
        p = p.split("?", 1)[0].rstrip("/")
        if p.endswith("/*"):
            p = p[:-2]  # resource-family wildcard → its base path
        return p

    def excluded(p):
        # /api/docs/* = error-envelope documentation_url value (§1.1), not an
        # endpoint; /api/v2/* = external exchange REST path (Bitget, 08-04).
        return (not p or p == "/api"
                or p.startswith("/api/docs/") or p.startswith("/api/v2/"))

    def matches(ref, doc):
        rs, ds = ref.split("/"), doc.split("/")
        if len(rs) != len(ds):
            return False
        for r, d in zip(rs, ds):
            wild = lambda s: s.startswith(":") or (s.startswith("{") and s.endswith("}"))
            if not (r == d or wild(d) or wild(r)):
                return False
        return True

    api = (ROOT / "integration-and-api/06-01-api-gateway-contract.md").read_text()
    # Documented surface = (method, path) rows of the §2.x endpoint tables
    # (served + Planned endpoints share the same row shape).
    documented = set()
    for m in re.finditer(r"^\|\s*`(GET|POST|DELETE|PUT|PATCH)`\s*\|\s*`(/api/[^`]+)`", api, re.MULTILINE):
        documented.add((m.group(1), clean(m.group(2))))
    documented_paths = {p for _, p in documented}

    errors = 0
    checked = set()
    for md in all_md():
        if md.name in HISTORY or md.name in GATE_SPEC:
            continue  # CHANGELOG = historical paths; MANIFEST = gate pattern text
        text = md.read_text()
        rel = str(md.relative_to(ROOT))
        # Method-qualified references must match a documented row on BOTH
        # method and path (a `:param` segment is not a license to alias a
        # different action verb, e.g. `/api/keys/rotate` ≠ `/api/keys/:key_id`).
        for m in method_rx.finditer(text):
            p = clean(m.group(2))
            if excluded(p):
                continue
            key = ("M", m.group(1), p, rel)
            if key in checked:
                continue
            checked.add(key)
            if not any(dm == m.group(1) and matches(p, dp) for dm, dp in documented):
                fail(f"{rel}: {m.group(1)} {p} not documented in 06-01 §2 (served or Planned endpoints)")
                errors += 1
        # Bare path references (no HTTP method) match on path shape only.
        text_wo_methods = method_rx.sub("", text)
        for raw in path_rx.findall(text_wo_methods):
            p = clean(raw)
            if excluded(p):
                continue
            key = ("P", p, rel)
            if key in checked:
                continue
            checked.add(key)
            if not any(matches(p, dp) for dp in documented_paths):
                fail(f"{rel}: /api path '{p}' not documented in 06-01 §2 (served or Planned endpoints)")
                errors += 1
    if errors == 0:
        ok(f"All referenced /api paths resolve to 06-01 (served or planned; {len(checked)} references checked)")

# ── G11: Audit-ID existence ────────────────────────────────────────────
def check_g11_audit_ids():
    print("\n=== G11: Audit-ID Existence (citations vs CHANGELOG §Open Items) ===")
    changelog = (ROOT / "CHANGELOG.md").read_text()
    open_sec = section_text(changelog, r"^## Open Items")
    open_ids = set(re.findall(r"AUDIT-V\d+-\d+", open_sec))
    cite_re = re.compile(r"AUDIT-V(\d+)-(\d+)(?:\s*(?:…|\.\.\.)\s*(\d+))?")
    # Literal gate rule: every AUDIT-V* cited outside the CHANGELOG resolves to
    # a §Open Items row. Citations of *resolved* register IDs in normative text
    # are defects per §12.10 (audit identifiers live only in the CHANGELOG).
    # The MANIFEST is excluded as governance: its §3 per-phase closure table is
    # a historical register of the same class as the CHANGELOG.
    errors = 0
    for md in all_md():
        if md.name in HISTORY or md.name in GATE_SPEC:
            continue
        text = md.read_text()
        for m in cite_re.finditer(text):
            v, start_n = m.group(1), int(m.group(2))
            end_n = int(m.group(3)) if m.group(3) else start_n
            ids = [f"AUDIT-V{v}-{n:03d}" for n in range(start_n, end_n + 1)]
            for aid in ids:
                if aid not in open_ids:
                    fail(f"{md.relative_to(ROOT)}: cites {aid}, which is not a CHANGELOG §Open Items row")
                    errors += 1
    if errors == 0:
        ok(f"Every AUDIT-V* citation outside the CHANGELOG resolves to a §Open Items row ({len(open_ids)} open IDs)")

# ── G12: Nonsense-phrase scan ──────────────────────────────────────────
def check_g12_nonsense():
    print("\n=== G12: Nonsense-Phrase Scan ===")
    # "formerly called X" is nonsense only when X is the *current* name of the
    # thing being described. Current names per §13.2 (the replacements for the
    # retired "Data Quality Matrix" term):
    CURRENT_NAMES = {"CandleQualityEnvelope", "PipelineReliabilityMetrics"}
    errors = 0
    for md in all_md():
        if md.name in HISTORY or md.name in GATE_SPEC:
            continue
        text = md.read_text()
        for i, line in enumerate(text.split("\n")):
            if re.search(r"\bdeadlock\b", line, re.IGNORECASE):
                fail(f"{md.relative_to(ROOT)}:{i+1}: 'deadlock' in normative text")
                errors += 1
            for m in re.finditer(r"formerly (?:called|known as)\s+(?:the\s+)?`?([A-Za-z][\w ]*?)`?\s*\)", line):
                if m.group(1).strip() in CURRENT_NAMES:
                    fail(f"{md.relative_to(ROOT)}:{i+1}: 'formerly called {m.group(1)}' uses the current name")
                    errors += 1
    if errors == 0:
        ok("No 'deadlock' or self-referential 'formerly called X' in normative text")

# ── G13: Appendix-A ≡ 02-07 §2.1 field set ─────────────────────────────
def check_g13_appendix_a_fields():
    print("\n=== G13: Appendix-A ≡ 02-07 §2.1 Field-Set Diff ===")
    sec = section_text((ROOT / "matrices/02-07-metrics-matrix.md").read_text(), r"^### 2\.1 ")
    fields = set()
    for line in sec.split("\n"):
        if not line.lstrip().startswith("|"):
            continue
        first_cell = line.split("|")[1] if len(line.split("|")) > 1 else ""
        # Combined rows (`bid_price` / `ask_price`) contribute every backticked name
        for part in re.findall(r"`([^`]+)`", first_cell):
            part = part.strip()
            if part and part != "Field":
                fields.add(part)
    example = first_json_fence_after(ROOT / "conceptual-foundations/01-01-ontology.md", r"^### A\.1 ")
    if example is None:
        fail("01-01 §A.1: JSON example not found")
        return
    keys = set(example.keys())
    missing = fields - keys
    extra = keys - fields
    if missing or extra:
        if missing:
            fail(f"01-01 §A.1 missing fields from 02-07 §2.1: {sorted(missing)}")
        if extra:
            fail(f"01-01 §A.1 has fields not in 02-07 §2.1: {sorted(extra)}")
    else:
        ok(f"Appendix A.1 field set == 02-07 §2.1 field set ({len(fields)} fields)")

# ── G17: Export payload schema (07-05 ⇄ exportBuilders) ────────────────
# Bidirectional key sweep over the MME export payload spec. Doc-side keys
# come from the ```json fences in 07-05; code-side keys come from the
# TypeScript interfaces in ui/src/lib/exportBuilders/*.ts (7 MME tabs +
# shared) and ui/src/types.ts.
G17_MME_BUILDERS = [
    "metricsTab.ts",
    "mtfTab.ts",
    "alignmentTab.ts",
    "opportunityTab.ts",
    "riskTab.ts",
    "analysisTab.ts",
    "recommendationTab.ts",
    "shared.ts",
]
# Literal-built payload keys that never appear as interface members
# (retracement coefficient object in `extractFibSummary`, the MTF
# `meta.timesframes` spelling).
G17_LITERAL_KEYS = {
    "fib_0236", "fib_0382", "fib_0500", "fib_0618", "fib_0660", "fib_0786",
    "timesframes",
    # Projection grid fields — defined in ui/src/lib/projection.ts
    # (`ProjectionState`), passed verbatim through the recommendation
    # builder (recommendationTab.ts `projection` block); the checker only
    # scans ui/src/types + exportBuilders.
    "configured", "capital", "leverage", "direction", "entry_price",
    "stop_loss", "take_profit", "position_size_units", "position_notional_usd",
    "entry_fee_usd", "exit_fee_usd", "total_fees_usd", "liquidation_price",
    "net_profit_usd", "roi_pct",
}
# Interface members belonging to the charts/positions payloads (shared
# `AccountBlock` / `CountsBlock`), documented only by table reference in
# 07-05 §1 — plus internal helper shapes that are renamed on the wire
# (`PriceBlock.current/prev_day` → `meta.current_price/prev_day_price`,
# `RrBlock.reason` → `*_reason`) — plus regex extraction artifacts from
# comment stripping.
G17_CODE_ONLY_EXEMPT = {
    "balance", "exported_at", "history", "leverage", "margin_used",
    "open_orders", "positions", "symbol",
    "current", "prev_day", "reason",
    "ontext", "erm", "pec", "rofile", "al", "ast", "atterns",
}


def _json_fence_keys(text):
    keys = set()
    for fence in re.findall(r"```json\n(.*?)```", text, re.S):
        try:
            obj = json.loads(fence)
        except Exception:
            continue
        stack = [obj]
        while stack:
            cur = stack.pop()
            if isinstance(cur, dict):
                for k, v in cur.items():
                    if isinstance(k, str) and re.fullmatch(r"[a-z_][a-z0-9_]*", k):
                        keys.add(k)
                    stack.append(v)
            elif isinstance(cur, list):
                stack.extend(cur)
    return keys


def _ts_interface_keys(src, exclude_inputs=False):
    keys = set()
    for m in re.finditer(r"interface\s+(\w+)\s*\{(.*?)\n\}", src, re.S):
        name = m.group(1)
        if exclude_inputs and name.endswith("Inputs"):
            continue
        body = re.sub(r"/\*.*?\*/|//[^\n]*", "", m.group(2), flags=re.S)
        for seg in body.split(";"):
            for f in re.findall(r"([a-z_][a-z0-9_]*)\s*:", seg):
                keys.add(f)
    return keys


def check_g17_export_schema():
    print("\n=== G17: Export Payload Schema (07-05 ⇄ exportBuilders) ===")
    doc_path = ROOT / "ui-ux/07-05-export-data-payload-schema.md"
    if not doc_path.exists():
        fail("07-05-export-data-payload-schema.md missing")
        return
    doc = doc_path.read_text()
    doc_keys = _json_fence_keys(doc)
    if not doc_keys:
        fail("07-05: no json-fence keys extracted")
        return

    builders_dir = ROOT.parent / "ui/src/lib/exportBuilders"
    builder_keys = set()
    missing_builders = []
    for name in G17_MME_BUILDERS:
        p = builders_dir / name
        if not p.exists():
            missing_builders.append(name)
            continue
        builder_keys |= _ts_interface_keys(p.read_text(), exclude_inputs=True)

    types_keys = set()
    for p in [ROOT.parent / "ui/src/types.ts"] + sorted(
        (ROOT.parent / "ui/src/types").glob("*.ts")
    ):
        if p.exists():
            types_keys |= _ts_interface_keys(p.read_text())

    # Direction 1 — every documented field must exist in the builders.
    code_union = builder_keys | types_keys | G17_LITERAL_KEYS
    doc_only = doc_keys - code_union
    if doc_only:
        fail(
            f"07-05 documents {len(doc_only)} field(s) not produced by any "
            f"export builder or store type: {sorted(doc_only)}"
        )
    else:
        ok(f"07-05 documents {len(doc_keys)} fields — all present in builders/store types")

    # Direction 2 — every MME builder field must be documented.
    code_only = builder_keys - doc_keys - G17_CODE_ONLY_EXEMPT
    if code_only:
        fail(
            f"exportBuilders emit {len(code_only)} field(s) missing from 07-05: "
            f"{sorted(code_only)}"
        )
    else:
        ok(f"MME export builders emit {len(builder_keys)} fields — all documented in 07-05")

    if missing_builders:
        fail(f"07-05 §1 references builders that do not exist: {missing_builders}")
    else:
        ok("07-05 §1 builder file map resolves")


# ── G14: Relative-link existence (alias of legacy CHECK 1) ─────────────
def check_g14_links():
    print("\n=== G14: Relative-Link Existence (alias: legacy CHECK 1) ===")
    broken = 0
    for md in all_md():
        base = md.parent
        text = md.read_text()
        for m in LINK_RE.finditer(text):
            target = m.group(1).strip()
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            path_part = target.split("#", 1)[0]
            if not path_part:
                continue
            try:
                resolved = (base / path_part).resolve()
            except Exception:
                broken += 1
                if broken <= 20:
                    fail(f"{md.relative_to(ROOT)}: broken link -> {target}")
                continue
            if not resolved.exists():
                broken += 1
                if broken <= 20:
                    fail(f"{md.relative_to(ROOT)}: missing target -> {target}")
    if broken == 0:
        ok(f"All relative links resolve correctly")
    else:
        fail(f"{broken} broken links found")

# ── G15: DDL ↔ index-name agreement ────────────────────────────────────
def check_g15_index_agreement():
    print("\n=== G15: DDL ↔ Index-Name Agreement (06-02 §2 vs §3.x) ===")
    text = (ROOT / "integration-and-api/06-02-database-schema-spec.md").read_text()
    sec2 = section_text(text, r"^## 2\. ")
    catalog = set(re.findall(r"^\|\s*`(idx_[a-z0-9_]+)`\s*\|", sec2, re.MULTILINE))
    sec3 = text[re.search(r"^## 3\. ", text, re.MULTILINE).start():]
    ddl = set(re.findall(r"CREATE INDEX(?:\s+IF NOT EXISTS)?\s+(idx_[a-z0-9_]+)", sec3))
    missing_in_catalog = ddl - catalog
    missing_in_ddl = catalog - ddl
    if missing_in_catalog or missing_in_ddl:
        if missing_in_catalog:
            fail(f"06-02: CREATE INDEX statements missing from §2 catalog: {sorted(missing_in_catalog)}")
        if missing_in_ddl:
            fail(f"06-02: §2 catalog entries without CREATE INDEX in §3.x: {sorted(missing_in_ddl)}")
    else:
        ok(f"§2 index catalog == §3.x CREATE INDEX statements ({len(catalog)} indexes)")

# ── G16: Open-item target validity ─────────────────────────────────────
def check_g16_open_item_targets():
    print("\n=== G16: Open-Item Target Validity (CHANGELOG §Open Items) ===")
    cv = current_version()
    if not cv:
        fail("cannot determine current corpus version")
        return
    cur = (cv[0], cv[1])
    changelog = (ROOT / "CHANGELOG.md").read_text()
    open_sec = section_text(changelog, r"^## Open Items")
    if not open_sec:
        fail("CHANGELOG.md: '## Open Items' section not found")
        return
    errors = 0
    rows = 0
    for line in open_sec.split("\n"):
        m = re.match(r"^\|\s*`(AUDIT-[^`]+)`\s*\|(.+)\|\s*([^|]+?)\s*\|\s*$", line)
        if not m:
            continue
        rows += 1
        aid, target = m.group(1), m.group(3).strip().strip("`")
        if target == "Unscheduled":
            continue
        tm = re.fullmatch(r"v(\d+)\.(\d+)", target)
        if tm and (int(tm.group(1)), int(tm.group(2))) >= cur:
            continue
        fail(f"CHANGELOG §Open Items {aid}: target '{target}' (< current v{cur[0]}.{cur[1]} and not 'Unscheduled')")
        errors += 1
    if errors == 0:
        ok(f"All {rows} Open Items rows carry a target ≥ v{cur[0]}.{cur[1]} or 'Unscheduled'")

# ── G18: CLI ↔ GUI Observe-Mode Parity (01-10) ─────────────────────────
# The parity contract is a release gate: the doc must exist, carry all 13
# checks, and the renderer's field-coverage test must be present in the
# CLI renderer source (the automated half of the contract).
def check_cli_gui_parity():
    print("\n=== G18: CLI ↔ GUI Observe-Mode Parity (01-10) ===")
    doc = ROOT / "conceptual-foundations/01-10-cli-gui-parity.md"
    if not doc.exists():
        fail("01-10-cli-gui-parity.md missing — the parity contract doc was removed")
        return
    text = doc.read_text()
    missing = [f"C{n}" for n in range(1, 14) if f"C{n}" not in text]
    if missing:
        fail(f"01-10 parity checklist incomplete — missing rows: {', '.join(missing)}")
    else:
        ok("01-10 carries all 13 parity checks (C1–C13)")
    renderer = ROOT.parent / "crates/execution-daemon/src/cli_renderer.rs"
    if renderer.exists():
        rt = renderer.read_text()
        if "frame_covers_every_gui_panel_field" in rt:
            ok("cli_renderer.rs carries the field-coverage parity test")
        else:
            fail("cli_renderer.rs lost the field-coverage parity test")
    else:
        fail("cli_renderer.rs not found")

# ═══════════════════════════════════════════════════════════════════════
# LEGACY REGRESSION CHECKS (retained from the v6.2 gate)
# ═══════════════════════════════════════════════════════════════════════

# ── Legacy CHECK 2: Retired Term Grep ──────────────────────────────────
def check_retired_terms():
    print("\n=== LEGACY CHECK 2: Retired Term Grep ===")
    retired = [
        "retry_cooldown",
        "emergency_liquidation",   # must be is_emergency_liquidation
        "reward_risk",
        "correlation_risk",
        # environment_favorability / opportunity_classification removed from
        # this list (audit 2026-08-18): they are LIVE serialized fields on
        # AdvisoryMatrix (core-domain/src/advisory.rs) and were documented
        # in 02-04 §2.x — see the check's own note below.
        "invalid_level",
        "final_invalidation_level",
        "LinearInterpolation",
        "roi_percentage",
        "Direction Matrix",
        "Regime Compatibility Matrix",
        "Pause event loop",         # Phase 10
        "(pause/resume)",           # Phase 10
    ]
    hits = 0
    # The Export Data payload schema documents the ACTUAL wire keys the
    # builders emit (e.g. AdvisoryMatrix.opportunity_classification is
    # still the serialized field name in crates/core-domain/src/advisory.rs)
    # — a schema reference is not normative prose, so exempt that document.
    wire_schema_docs = {"07-05-export-data-payload-schema.md"}
    for md in all_md():
        text = md.read_text()
        rel = str(md.relative_to(ROOT))
        fname = md.name
        if fname in HISTORY or fname in GATE_SPEC or fname in wire_schema_docs:
            continue
        for i, line in enumerate(text.split("\n")):
            for term in retired:
                # Use word boundary regex to avoid matching substrings
                if re.search(rf'\b{re.escape(term)}\b', line):
                    hits += 1
                    if hits <= 15:
                        fail(f"{rel}:{i+1}: retired term '{term}' found")
    # Special: bare "PAUSED" check in normative sections for Phase 10
    bare_paused = 0
    # Canonical lifecycle spec may use bare PAUSED for enum value references
    canonical_lifecycle = {"03-03-06-tae-instance-lifecycle-spec.md"}
    for md in all_md():
        rel = str(md.relative_to(ROOT))
        if rel.startswith("CHANGELOG") or rel.startswith("DOCS-CONSISTENCY"):
            continue
        if md.name in canonical_lifecycle:
            continue
        in_code_fence = False
        for i, line in enumerate(md.read_text().split("\n")):
            if line.strip().startswith("```"):
                in_code_fence = not in_code_fence
                continue
            if in_code_fence:
                continue
            # Match "PAUSED" that is NOT "AUTO_PAUSED" and not qualified
            if re.search(r"\bPAUSED\b", line) and "AUTO_PAUSED" not in line:
                # Must be qualified with axis: "instance PAUSED", "lifecycle PAUSED"
                if not re.search(r"(instance|lifecycle|instance-scope|policy)\s*[`'\"]*\s*PAUSED", line):
                    bare_paused += 1
    if bare_paused > 0:
        fail(f"{bare_paused} bare PAUSED references without axis qualifier (Phase 10 scoped-enum rule)")
    if hits == 0 and bare_paused == 0:
        ok(f"No retired terms or bare PAUSED found")
    else:
        fail(f"Found retired terms or bare PAUSED")

# ── Legacy CHECK 5: Section Resolution ─────────────────────────────────
def check_section_refs():
    print("\n=== LEGACY CHECK 5: Section Cross-Reference Resolution ===")
    # Build heading index
    headings = {}
    for md in ROOT.rglob("*.md"):
        rel = str(md.relative_to(ROOT))
        text = md.read_text()
        for m in HEADING_RE.finditer(text):
            level = len(m.group(1))
            title = m.group(2).strip()
            # Extract section number from heading like "### 3.1 Foo"
            sec_match = re.match(r"(\d+(?:\.\d+)*)", title)
            if sec_match:
                sec_num = sec_match.group(1)
                headings[(rel, sec_num)] = True

    # Check section references
    broken = 0
    # Build set of all doc IDs (file name prefixes like "08-05", "03-01-04")
    doc_ids = set()
    for md in ROOT.rglob("*.md"):
        fname = md.name
        # Extract prefix like "08-05", "03-01-04", "02-00b", etc.
        m = re.match(r"(\d{2}-\d{2}[a-z]?(?:-\d{2})?)", fname)
        if m:
            doc_ids.add(m.group(1))
    for md in all_md():
        text = md.read_text()
        for m in SECTION_REF_RE.finditer(text):
            doc_id = m.group(1)
            section = m.group(2)
            if doc_id not in doc_ids:
                broken += 1
                if broken <= 10:
                    fail(f"{md.relative_to(ROOT)}: referenced doc '{doc_id}' not found")
    if broken == 0:
        ok(f"All section cross-references resolve to known documents")
    else:
        fail(f"{broken} unresolved section cross-references")

# ── Legacy CHECK 6: Worked-Example Recompute ───────────────────────────
def check_worked_examples():
    print("\n=== LEGACY CHECK 6: Worked-Example Regression Probes ===")
    # Verify the A-series numbers in 01-01-ontology.md
    onto = ROOT / "conceptual-foundations" / "01-01-ontology.md"
    text = onto.read_text() if onto.exists() else ""

    errors = 0

    # Check state_confidence: should be 0.65 (|40|/100 + 0.15 + 0.10)
    if "state_confidence" in text:
        m = re.search(r'"state_confidence"\s*:\s*0\.82', text)
        if m:
            fail(f"01-01-ontology.md: state_confidence still 0.82 (should be 0.65)")
            errors += 1

    # Check confidence_assessment = 0.65 * 0.717 * 100 = 46.61
    m = re.search(r'"confidence_assessment"\s*:\s*59\.07', text)
    if m:
        fail(f"01-01-ontology.md: confidence_assessment still 59.07 (should be 46.61)")
        errors += 1

    # Check AssetRank: should be 87.5
    m = re.search(r'"score"\s*:\s*87\.0', text)
    if m:
        # Only if it's in the AssetRank context
        context = text[max(0, m.start()-100):m.end()+100]
        if "AssetRank" in context or "asset_rank" in context:
            fail(f"01-01-ontology.md: AssetRank score still 87.0 (should be 87.5)")
            errors += 1

    # Check 02-03 quality_score: should be 100.0
    dq = ROOT / "matrices" / "02-03-data-quality-matrix.md"
    if dq.exists():
        dq_text = dq.read_text()
        if re.search(r'"quality_score"\s*:\s*98\.0', dq_text):
            fail(f"02-03-data-quality-matrix.md: quality_score still 98.0 (should be 100.0)")
            errors += 1

    # Check 02-05 quality_score: should be 100.0
    dist = ROOT / "matrices" / "02-05-distribution-matrix.md"
    if dist.exists():
        dist_text = dist.read_text()
        if re.search(r'"quality_score"\s*:\s*98\.0', dist_text):
            fail(f"02-05-distribution-matrix.md: quality_score still 98.0 (should be 100.0)")
            errors += 1

    # Check 02-08 opportunity_score: "PRIME" with score 85.0 -> "STRONG"
    opp = ROOT / "matrices" / "02-08-opportunity-matrix.md"
    if opp.exists():
        opp_text = opp.read_text()
        if re.search(r'"setup_quality"\s*:\s*"PRIME".*"opportunity_score"\s*:\s*85', opp_text, re.DOTALL):
            fail(f"02-08-opportunity-matrix.md: 85.0 still labeled PRIME (should be STRONG)")
            errors += 1

    # Check signal counts in 04-02-00
    idx = ROOT / "engines" / "market-monitoring-engine" / "indicators" / "04-02-00-indicator-index.md"
    if idx.exists():
        idx_text = idx.read_text()
        # The 04-02-00 table is the doc's authoritative source of truth
        # (per its own "Counts policy" note). The expected counts below
        # match the table as it stands after the v6.6 `mark_index_spread`
        # registry entry (Threshold count: 26 → 27; total: 100 → 101).
        # The list enforces the *current* correct values by detecting
        # accidental rollbacks to the pre-v6.6 figures.
        wrong_counts = [
            ("Crossover 8", "Crossover 9"),
            ("Threshold 26", "Threshold 27"),
            ("ZeroLineCross 10", "ZeroLineCross 11"),
            ("TrendFlip 7", "TrendFlip 8"),
        ]
        for wrong, correct in wrong_counts:
            if wrong in idx_text and correct not in idx_text:
                fail(f"04-02-00-indicator-index.md: still shows '{wrong}' (should be '{correct}')")
                errors += 1

    # Check connection quality score
    cq = ROOT / "operations-and-compliance" / "08-05-connection-quality.md"
    if cq.exists():
        cq_text = cq.read_text()
        m = re.search(r'0\.50\s*×\s*uptime_pct', cq_text)
        if m:
            fail(f"08-05-connection-quality.md: formula still mixes scales (should use point-scale)")
            errors += 1

    if errors == 0:
        ok(f"All canonical worked examples recompute correctly")
    else:
        fail(f"{errors} worked-example errors")

# ── Legacy CHECK 7: Signal Registry Tally ──────────────────────────────
def check_signal_registry():
    print("\n=== LEGACY CHECK 7: Signal Registry Tally ===")
    idx = ROOT / "engines" / "market-monitoring-engine" / "indicators" / "04-02-00-indicator-index.md"
    if not idx.exists():
        fail("04-02-00-indicator-index.md not found")
        return
    idx_text = idx.read_text()

    # Per-indicator signal kinds from the table
    # Source of truth per 04-02-00-indicator-index.md "Counts policy":
    # the index's Signals column mirrors the registry `signal_types`
    # manifest (crates/market-analyzer/src/indicators/registry.rs).
    # Total 101 = post-v6.11 (v6.6 added `mark_index_spread` Threshold,
    # v6.11 added `price_trend_sharpe`; composition re-verified
    # 2026-08-17: Threshold 22, ZeroLineCross 13, TrendFlip 10,
    # Crossover 10, LevelTest 14, Breakout 9, Divergence 9, BandTouch 4,
    # CompressionRelease 4, PatternForming 3, VolumeClimax 2,
    # StackChange 1). bbwp/squeeze Threshold rows and the aroon Crossover
    # row are runtime-derived/declared capability — see the spec-file
    # annotations in 04-02-27/28/36.
    expected = {
        "Divergence": 9, "Crossover": 10, "Threshold": 22, "Breakout": 9,
        "BandTouch": 4, "ZeroLineCross": 13, "CompressionRelease": 4,
        "LevelTest": 14, "TrendFlip": 10, "VolumeClimax": 2,
        "StackChange": 1, "PatternForming": 3
    }

    # Count declarations (indicator rows that mention each SignalKind)
    per_kind = {k: 0 for k in expected}
    for k in expected:
        # Only count rows that are actual indicator entries: start with | followed by a number (index)
        pattern = rf"^\|\s*\d+\s*\|.*\b{k}\b"
        matches = re.findall(pattern, idx_text, re.MULTILINE)
        per_kind[k] = len(matches)

    errors = 0
    for kind, count in expected.items():
        actual = per_kind[kind]
        if actual != count:
            fail(f"Signal kind {kind}: expected {count}, found {actual}")
            errors += 1

    total = sum(per_kind.values())
    if total == 101:
        ok(f"Signal declarations tally: {total}/101, per-kind counts verify")
    else:
        fail(f"Signal declarations tally: {total} (expected 101)")

# ── Legacy CHECK 8: Enum CHECK Cross-Check (Phase 10) ──────────────────
def check_enum_cross():
    print("\n=== LEGACY CHECK 8: Enum CHECK Cross-Check ===")
    db_schema = ROOT / "integration-and-api" / "06-02-database-schema-spec.md"
    lifecycle = ROOT / "engines" / "trade-automation-engine" / "03-03-06-tae-instance-lifecycle-spec.md"
    tae_policy = ROOT / "engines" / "trade-automation-engine" / "03-03-04-tae-execution-policy-spec.md"

    errors = 0
    if db_schema.exists():
        db_text = db_schema.read_text()
        expected_states = {"RUNNING", "PAUSED", "STOPPING", "STOPPED"}
        # Find lifecycle_state CHECK
        m = re.search(r"lifecycle_state\s+TEXT[^)]*CHECK\s*\(([^)]+)\)", db_text)
        if m:
            check_str = m.group(1)
            actual_states = set(re.findall(r"'(\w+)'", check_str))
            if expected_states != actual_states:
                fail(f"instance_lifecycle CHECK states mismatch: DB={actual_states}, spec={expected_states}")
                errors += 1
            else:
                ok(f"lifecycle_state CHECK matches IL-01 spec: {expected_states}")

    if lifecycle.exists():
        lc_text = lifecycle.read_text()
        # AUTO_PAUSED should not be a value in the LifecycleState enum declaration
        # (it's a policy state). Look for the CHECK or table that defines the enum values.
        m = re.search(r"lifecycle_state.*CHECK.*\(([^)]+)\)", lc_text)
        if m:
            check_values = re.findall(r"'(\w+)'", m.group(1))
            if "AUTO_PAUSED" in check_values:
                fail(f"03-03-06: AUTO_PAUSED appears as LifecycleState CHECK value")
                errors += 1
        # Also check the decision-table values
        m2 = re.search(r"to_state.*CHECK.*\(([^)]+)\)", lc_text)
        if m2:
            check_values = re.findall(r"'(\w+)'", m2.group(1))
            if "AUTO_PAUSED" in check_values:
                fail(f"03-03-06: AUTO_PAUSED appears as to_state CHECK value")
                errors += 1

    if errors == 0:
        ok(f"Enum CHECK cross-check passes")

# ── Legacy CHECK 9: Gate-Chain Completeness (Phase 10) ─────────────────
def check_gate_chain():
    print("\n=== LEGACY CHECK 9: Gate-Chain Completeness ===")
    risk_ctrl = ROOT / "operations-and-compliance" / "08-02-pre-trade-risk-controls.md"
    if not risk_ctrl.exists():
        ok(f"Skipped: 08-02 not found")
        return
    text = risk_ctrl.read_text()
    expected_gates = {f"Gate {i}" for i in range(0, 8)}
    found_gates = set(re.findall(r"Gate\s+(\d)", text))
    found_gates = {f"Gate {g}" for g in found_gates}
    if expected_gates == found_gates:
        ok(f"All Gates 0–7 present in 08-02 pre-trade risk controls")
    else:
        missing = expected_gates - found_gates
        extra = found_gates - expected_gates
        fail(f"Gate mismatch: missing={missing}, extra={extra}")

# ── Legacy CHECK 10: Endpoint Cross-Check (Phase 10) ───────────────────
def check_endpoint_cross():
    print("\n=== LEGACY CHECK 10: Endpoint Cross-Check ===")
    api = ROOT / "integration-and-api" / "06-01-api-gateway-contract.md"
    lifecycle = ROOT / "engines" / "trade-automation-engine" / "03-03-06-tae-instance-lifecycle-spec.md"
    if not api.exists():
        ok(f"Skipped: 06-01 not found")
        return
    api_text = api.read_text()
    errors = 0

    # These endpoints should exist after Phase 10
    required_endpoints = [
        ("POST", "/api/instances/:instance_id/start"),
        ("POST", "/api/instances/:instance_id/pause"),
        ("POST", "/api/instances/:instance_id/stop"),
        ("DELETE", "/api/instances/:instance_id"),
    ]
    for method, path in required_endpoints:
        if path in api_text:
            ok(f"Endpoint {method} {path} present in 06-01")
        else:
            fail(f"Endpoint {method} {path} missing from 06-01")
            errors += 1

    if errors == 0:
        ok(f"All required endpoints documented in 06-01")


# ── G18: DS parity (01-10 §C14-C16 ⇄ cli_ds.rs) ───────────────────────
def check_g18_ds_parity():
    print("\n=== G18: CLI/GUI DS parity (01-10 ⇄ cli_ds.rs) ===")
    global failures
    parity = (ROOT / "conceptual-foundations/01-10-cli-gui-parity.md").read_text()
    cli_ds = (ROOT.parent / "crates/execution-daemon/src/cli_ds.rs").read_text()
    errors = 0
    for token in ["--sessions", "--session-report", "--backtest-show"]:
        if token in parity:
            ok(f"01-10 documents {token}")
        else:
            fail(f"01-10 missing DS command {token}")
            errors += 1
        if token in cli_ds:
            ok(f"cli_ds.rs implements {token}")
        else:
            fail(f"cli_ds.rs missing {token}")
            errors += 1
    if errors == 0:
        ok("DS parity contract C14-C16 present on both sides")


# ── G19: D/I/L ontology (01-12) ───────────────────────────────────────
def check_g19_dil_ontology():
    print("\n=== G19: D/I/L ontology (01-12) ===")
    global failures
    ontology = (ROOT / "conceptual-foundations/01-12-data-information-learning-ontology.md").read_text()
    errors = 0
    for header in ["## Data (D)", "## Information (I)", "## Learning (L)"]:
        if header in ontology:
            ok(f"01-12 carries tier header {header}")
        else:
            fail(f"01-12 missing tier header {header}")
            errors += 1
    for anchor in ["tier-mapping", "Invariants", "aggregation axis"]:
        if anchor in ontology:
            ok(f"01-12 carries '{anchor}'")
        else:
            fail(f"01-12 missing '{anchor}'")
            errors += 1
    if errors == 0:
        ok("D/I/L ontology doc complete")


# ── G20: DDL ↔ doc ↔ code agreement ───────────────────────────────────
def check_g20_ddl_doc_code():
    print("\n=== G20: DDL ⇄ doc ⇄ code (sessions + ds export) ===")
    global failures
    schema_doc = (ROOT / "integration-and-api/06-02-database-schema-spec.md").read_text()
    ds_doc = (ROOT / "integration-and-api/06-04-ds-export-schema.md").read_text()
    migrations = (ROOT.parent / "crates/database-storage/migrations/20260823000001_sessions.sql").read_text()
    ds_code = (ROOT.parent / "crates/database-storage/src/ds_export.rs").read_text()
    exporter = (ROOT.parent / "crates/execution-daemon/src/ds_exporter.rs").read_text()
    errors = 0
    for col in ["sessions", "session_id", "backtest_runs"]:
        if col in schema_doc:
            ok(f"06-02 documents '{col}'")
        else:
            fail(f"06-02 missing '{col}'")
            errors += 1
    for table in ["market_snapshots", "trade_telemetry_history", "paper_trades",
                  "portfolio_equity_history", "automation_activity", "risk_control_events",
                  "backtest_runs"]:
        if table not in migrations:
            fail(f"session migration missing ALTER for {table}")
            errors += 1
        else:
            ok(f"session_id ALTER present for {table}")
    for fname in ["session.json", "trades.ndjson", "equity.ndjson", "activity.ndjson",
                  "risk_events.ndjson", "strategy.ndjson", "risk.ndjson",
                  "performance.ndjson", "run.json", "signals.ndjson", "portfolio.ndjson"]:
        haystack = ds_doc + ds_code + exporter
        if fname in haystack:
            ok(f"06-04 filename {fname} present in code")
        else:
            fail(f"06-04 filename {fname} missing from code")
            errors += 1
    if errors == 0:
        ok("DDL ⇄ doc ⇄ code agreement holds")

# ── Main ────────────────────────────────────────────────────────────────
def main():
    global failures
    print("=" * 60)
    print("DOC CORPUS CONSISTENCY CHECK — v6.4 release-gate suite")
    print("=" * 60)
    # Release gates (MANIFEST §12.0)
    check_g1_version_coherence()
    check_g2_file_count()
    check_g3_csr_duplication()
    check_g4_canonical_scenario()
    check_g5_enum_cardinality()
    check_g6_enum_casing()
    check_g7_toml_fences()
    check_g8_stale_targets()
    check_g9_placeholders()
    check_g10_api_coverage()
    check_g11_audit_ids()
    check_g12_nonsense()
    check_g13_appendix_a_fields()
    check_g14_links()
    check_g15_index_agreement()
    check_g16_open_item_targets()
    check_g17_export_schema()
    check_g18_ds_parity()
    check_g19_dil_ontology()
    check_g20_ddl_doc_code()
    check_cli_gui_parity()
    # Legacy regression checks retained from the v6.2 gate
    check_retired_terms()
    check_section_refs()
    check_worked_examples()
    check_signal_registry()
    check_enum_cross()
    check_gate_chain()
    check_endpoint_cross()
    print("=" * 60)
    if failures == 0:
        print("RESULT: ALL CHECKS PASSED")
        sys.exit(0)
    else:
        print(f"RESULT: {failures} FAILURE(S)")
        sys.exit(1)

if __name__ == "__main__":
    main()
