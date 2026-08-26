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

/// Cores to give the INCUMBENT arm, as a set.
///
/// Defect 1 of bd-hmfi: only the frankenmermaid arm was pinned. The mermaid-js arm ran under
/// Chromium across all 64 cores, so the two arms were not merely on different cores, they were under
/// different clock regimes — ours parked at the DVFS floor by the old selection rule, theirs boosted
/// by its own load. Fixing our side alone (52b72e34) removed half the asymmetry.
///
/// ⚠️ THE SIZING ERROR HERE IS ASYMMETRIC AND ONLY ONE DIRECTION IS SAFE. Chromium is multi-process;
/// constraining it to too few cores would SLOW THE INCUMBENT and inflate our ratio — an over-claim in
/// our own favour, which is far worse than the under-claim the old code produced. So the set is
/// generous by default and never smaller than `min(floor, band)`: when in doubt the incumbent gets
/// more cores, not fewer.
///
/// Returns the fastest `size` cores from the idle band, so both arms are drawn from the same
/// population by the same rule and differ in count only because one is single-threaded and the other
/// is a browser.
/// ⚠️ SELECTING BY RANK MADE THE ARMS UNCOMPARABLE IN THE OTHER DIRECTION, and `targetMhz` is the
/// fix. Taking the fastest N necessarily reaches DOWN as N grows: measured live, the single-core arm
/// took 3433 MHz while the eight-core incumbent set spanned 1916-4292 MHz, a 1.79x internal range.
/// A browser scheduled onto the slow member runs slower than it would UNPINNED, which inflates the
/// ratio in our own favour -- the direction this function's own warning calls the worse error.
///
/// With `targetMhz` supplied the set is chosen by CLOSENESS to the measured arm's clock instead of by
/// rank. That has no free parameter to argue about: the objective is "cores like the one our arm got",
/// not "cores above some threshold I picked". Without it the old rank behaviour is kept, so existing
/// callers are unaffected.
///
export function selectPinnedCpuSet(records, size = 8, targetMhz = null) {
  if (!Array.isArray(records) || records.length === 0) {
    throw new Error('selectPinnedCpuSet requires at least one cpu record');
  }
  if (!Number.isInteger(size) || size < 1) {
    throw new Error('selectPinnedCpuSet size must be a positive integer');
  }
  const busy = [...records].sort((a, b) => a.busy - b.busy);
  const quietest = busy[0].busy;
  const band = busy.filter((record) => record.busy <= quietest + 0.05);
  const clocked = band.every((record) => typeof record.mhz === 'number' && record.mhz > 0);
  let ranked = band;
  if (clocked) {
    ranked = typeof targetMhz === 'number' && targetMhz > 0
      ? [...band].sort((a, b) => Math.abs(a.mhz - targetMhz) - Math.abs(b.mhz - targetMhz))
      : [...band].sort((a, b) => b.mhz - a.mhz);
  }
  const chosen = ranked.slice(0, Math.min(size, ranked.length));
  return {
    cpus: chosen.map((record) => record.cpu),
    band_size: band.length,
    requested: size,
    // Recorded so a row states plainly whether the incumbent got the cores it asked for. A short set
    // is the case that would bias in our favour, and it must be visible rather than inferred.
    starved: chosen.length < size,
    min_mhz: chosen.length > 0 && typeof chosen[0].mhz === 'number'
      ? Math.min(...chosen.map((record) => record.mhz))
      : null,
    // The set's MEAN clock, which is what a comparability check has to use: the incumbent runs
    // across all of these cores, so its effective clock is not the slowest one. Comparing our single
    // pinned core against `min_mhz` overstates the gap; against the mean it does not.
    mean_mhz: chosen.length > 0 && clocked
      ? Math.round(chosen.reduce((total, record) => total + record.mhz, 0) / chosen.length)
      : null,
    // The widest clock gap inside the chosen set, so a row states how comparable its incumbent
    // cores actually were instead of leaving it to be inferred from the rule name.
    spread: chosen.length > 0 && clocked
      ? Number(
          (Math.max(...chosen.map((r) => r.mhz)) / Math.max(1, Math.min(...chosen.map((r) => r.mhz))))
            .toFixed(3),
        )
      : null,
    rule: !clocked
      ? 'idle_band_no_cpufreq'
      : typeof targetMhz === 'number' && targetMhz > 0
        ? 'clocks_closest_to_measured_arm'
        : 'fastest_clocks_in_idle_band',
  };
}

/// How far below the host's peak clock the measured arm actually ran.
///
/// ⚠️ THIS IS A DISCLOSURE, NOT A FIX, and the distinction is the point. Our arm is pinned to an IDLE
/// core, and under per-core DVFS idle cores are BY DEFINITION the parked ones -- measured live,
/// 2397 MHz against a host maximum of 4217. So the measured arm systematically runs below the clock a
/// busy, boosted core would give it, and every ratio we quote UNDERSTATES our engine. That direction
/// is conservative, so no banked row is threatened, but until now the quantity appeared in no row at
/// all.
///
/// Fixing it means either warming the target core before measuring or accepting the floor -- both
/// judgements about method that belong to whoever owns the row, not to the code. Reporting it is
/// neither: it just stops the number being invisible.
///
/// `ratio` is chosen/peak, so 1.0 means the arm got the fastest clock on the box and 0.57 means it
/// ran at 57% of it. `at_peak` is true only when the arm is within 2% of the maximum, which is the
/// case where the caveat can honestly be omitted from a row.
export function clockHeadroom(chosenMhz, hostMaxMhz) {
  if (
    typeof chosenMhz !== 'number' ||
    typeof hostMaxMhz !== 'number' ||
    chosenMhz <= 0 ||
    hostMaxMhz <= 0
  ) {
    return { ratio: null, at_peak: null, note: 'cpufreq unavailable, clock headroom unknown' };
  }
  const ratio = Number((chosenMhz / hostMaxMhz).toFixed(3));
  const at_peak = ratio >= 0.98;
  return {
    ratio,
    at_peak,
    note: at_peak
      ? 'measured arm ran at the host peak clock'
      : `measured arm ran at ${Math.round(ratio * 100)}% of the host peak (${chosenMhz} of ${hostMaxMhz} MHz); the ratio understates this engine`,
  };
}
