/// Which core the measured arm gets pinned to.
///
/// Split out of `run.mjs` so the decision can be exercised directly: `run.mjs` parses argv at import
/// time, so nothing inside it is reachable from a check without running the whole driver.
///
/// ⚠️ THE RULE THIS REPLACES TOOK THE LEAST-BUSY CORE, WHICH ON PER-CORE DVFS IS THE SLOWEST ONE
/// (bd-hmfi). Idle cores are parked at the frequency floor: measured live on thinkstation1,
/// cpu10/15/24/27/28 sat at 1429 MHz while cpu34 ran at 3914 MHz — 2.739x apart at the same instant,
/// against a governor range of 1429–4562 MHz. Sorting by busy fraction and taking the minimum
/// therefore actively preferred the floor for the frankenmermaid arm, while the mermaid-js arm runs
/// unpinned across all cores and its own load boosts whichever it lands on. Neither the 20%
/// quiescence veto nor the per-engine A/A null can see this: occupancy is not speed, and a null
/// measured inside one phase proves self-consistency at that phase's frequency, not comparable
/// clocks between phases.
///
/// Idleness remains a REQUIREMENT — a core with a co-tenant makes the measurement meaningless — so
/// speed only breaks ties among cores that were already acceptable on occupancy grounds.
///
/// `records` are `{cpu, busy, mhz}`, `busy` a fraction in [0,1], `mhz` a number or null.
export function selectPinnedCpu(records) {
  if (!Array.isArray(records) || records.length === 0) {
    throw new Error('selectPinnedCpu requires at least one cpu record');
  }
  const busy = [...records].sort((a, b) => a.busy - b.busy);
  const quietest = busy[0].busy;
  // The band is anchored to the quietest core actually OBSERVED rather than an absolute constant, so
  // this can never start preferring a loaded core on a busy host: if every core sits at 40%, the band
  // is 40–45% and the run is refused by the quiescence gate regardless of which member is picked.
  const band = busy.filter((record) => record.busy <= quietest + 0.05);
  const withMhz = band.filter((record) => typeof record.mhz === 'number' && record.mhz > 0);
  if (withMhz.length === 0) {
    // No cpufreq: behave exactly as before rather than failing, so a host without per-core DVFS is
    // unaffected by this change.
    return { chosen: busy[0], band_size: band.length, rule: 'least_busy_no_cpufreq' };
  }
  const chosen = withMhz.reduce((best, record) => (record.mhz > best.mhz ? record : best));
  return { chosen, band_size: band.length, rule: 'fastest_clock_among_idle' };
}
