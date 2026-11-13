#!/usr/bin/env bash
# Provision a Boat Ubuntu worker for native KiCad tests. No project data or
# credentials belong in this system-tool setup, so it is safe to snapshot later.
set -euo pipefail

source /etc/os-release
if [[ "$ID" != ubuntu || "$VERSION_ID" != 24.04 || "$(uname -m)" != x86_64 ]]; then
    printf '%s\n' 'This setup supports Ubuntu 24.04 x86_64 only.' >&2
    exit 1
fi

python3 - <<'PY'
from pathlib import Path
import os
import shutil

available = int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines()
                     if line.startswith('MemAvailable:'))) * 1024
free_disk = shutil.disk_usage('/').free
print({'available_memory_bytes': available, 'free_disk_bytes': free_disk,
       'load_average': os.getloadavg()}, flush=True)
# Budget 1 GB for package installation and retain at least the 500 MB reserve.
if available < 1_500_000_000 or free_disk < 5_000_000_000:
    raise SystemExit('Insufficient capacity for bounded KiCad provisioning')
PY

packages=(kicad kicad-symbols kicad-footprints protobuf-compiler libprotobuf-dev xvfb xauth)
ready=true
for package in "${packages[@]}"; do
    if ! dpkg-query -W -f='${db:Status-Status}' "$package" 2>/dev/null | grep -qx installed; then
        ready=false
    fi
done
if ! command -v kicad-cli >/dev/null || [[ "$(kicad-cli --version)" != 10.* ]]; then
    ready=false
fi

if [[ "$ready" != true ]]; then
    sudo -n true
    if ! command -v add-apt-repository >/dev/null; then
        sudo -n apt-get update
        sudo -n env DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l \
            apt-get install -y --no-install-recommends software-properties-common
    fi
    if ! grep -rqs 'kicad-10.0-releases' /etc/apt/sources.list.d; then
        sudo -n add-apt-repository -y --no-update ppa:kicad/kicad-10.0-releases
    fi
    sudo -n apt-get update
    candidate=$(apt-cache policy kicad | awk '/Candidate:/ {print $2}')
    if [[ "$candidate" != 10.* ]]; then
        printf 'Refusing unsupported KiCad candidate: %s\n' "$candidate" >&2
        exit 1
    fi
    # --no-install-recommends excludes the multi-gigabyte kicad-packages3d set.
    sudo -n env DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l \
        apt-get install -y --no-install-recommends "kicad=$candidate" \
        kicad-symbols kicad-footprints protobuf-compiler libprotobuf-dev xvfb xauth
fi

version=$(kicad-cli --version)
if [[ "$version" != 10.* ]]; then
    printf 'Expected KiCad 10.x, observed %s\n' "$version" >&2
    exit 1
fi
printf 'KiCad CLI: %s\n' "$version"
protoc --version
protoc -I /usr/include --descriptor_set_out=/dev/null google/protobuf/any.proto
dpkg-query -W -f='${Package} ${Version}\n' "${packages[@]}"
/usr/bin/python3 - <<'PY'
import pcbnew
print('pcbnew Python:', pcbnew.Version())
PY
