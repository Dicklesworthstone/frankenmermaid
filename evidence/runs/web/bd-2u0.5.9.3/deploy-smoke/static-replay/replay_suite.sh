#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/../../../../.." && pwd)
cd "$repo_root"

# Rerun the full React release-grade suite
python3 scripts/run_static_web_e2e.py --bead-id bd-2u0.5.9.3-static --repo-root /data/projects/frankenmermaid --chromium /snap/bin/chromium --timeout-seconds 8 --output-root /data/projects/frankenmermaid/evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static --repeat 2 --route-prefix /web --surface web --host-kind static-web --scenario-prefix static-web --serve-root /data/projects/frankenmermaid/dist/cloudflare-pages/bd-2u0.5.9.3-stage --revision b01fe3802567360415340b8d9b0bd870fbf2c9c8

# Replay a single case by uncommenting one command below
# static-web-compare-export / desktop-default
# python3 scripts/run_static_web_e2e.py --bead-id bd-2u0.5.9.3-static --repo-root /data/projects/frankenmermaid --chromium /snap/bin/chromium --timeout-seconds 8 --output-root /data/projects/frankenmermaid/evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static --repeat 1 --route-prefix /web --surface web --host-kind static-web --scenario-prefix static-web --serve-root /data/projects/frankenmermaid/dist/cloudflare-pages/bd-2u0.5.9.3-stage --revision b01fe3802567360415340b8d9b0bd870fbf2c9c8 --scenario-id static-web-compare-export --profile-id desktop-default
# static-web-diagnostics-recovery / desktop-default
# python3 scripts/run_static_web_e2e.py --bead-id bd-2u0.5.9.3-static --repo-root /data/projects/frankenmermaid --chromium /snap/bin/chromium --timeout-seconds 8 --output-root /data/projects/frankenmermaid/evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static --repeat 1 --route-prefix /web --surface web --host-kind static-web --scenario-prefix static-web --serve-root /data/projects/frankenmermaid/dist/cloudflare-pages/bd-2u0.5.9.3-stage --revision b01fe3802567360415340b8d9b0bd870fbf2c9c8 --scenario-id static-web-diagnostics-recovery --profile-id desktop-default
