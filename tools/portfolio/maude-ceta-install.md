# Maude and CeTA Portfolio Tool Install

Consolidated build: see `tools/portfolio/Dockerfile` and `tools/portfolio/README.md`.

This appendix pins a Debian-based image setup for the Maude equational backend and the certified termination and confluence gate. It does not modify any Dockerfile.

Pinned versions:

- Debian: bookworm
- Maude: 3.5.1
- Java: Eclipse Temurin 25 JRE
- AProVE: `master_2026_02_15`
- CeTA: 2.46
- CSI: 1.2.7
- Wanda: 0.6.1
- TTT2 alternate: 1.26

Install base packages:

```sh
set -eux
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  gpg \
  unzip \
  xz-utils \
  tar \
  bash \
  coreutils
rm -rf /var/lib/apt/lists/*
install -d /opt/provekit-tools/bin
```

Install Java 25 for AProVE:

```sh
set -eux
install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://packages.adoptium.net/artifactory/api/gpg/key/public \
  | gpg --dearmor -o /etc/apt/keyrings/adoptium.gpg
echo "deb [signed-by=/etc/apt/keyrings/adoptium.gpg] https://packages.adoptium.net/artifactory/deb bookworm main" \
  > /etc/apt/sources.list.d/adoptium.list
apt-get update
apt-get install -y --no-install-recommends temurin-25-jre
rm -rf /var/lib/apt/lists/*
java -version
```

Install Maude 3.5.1:

```sh
set -eux
install -d /opt/provekit-tools/maude-3.5.1
curl -fL \
  -o /tmp/maude-3.5.1-linux-x86_64.zip \
  https://github.com/maude-lang/Maude/releases/download/Maude3.5.1/Maude-3.5.1-linux-x86_64.zip
unzip -q /tmp/maude-3.5.1-linux-x86_64.zip -d /opt/provekit-tools/maude-3.5.1
ln -sf /opt/provekit-tools/maude-3.5.1/maude.linux64 /opt/provekit-tools/bin/maude
/opt/provekit-tools/bin/maude --version
```

Install AProVE `master_2026_02_15`:

```sh
set -eux
install -d /opt/provekit-tools/aprove-master_2026_02_15
curl -fL \
  -o /opt/provekit-tools/aprove-master_2026_02_15/aprove.jar \
  https://github.com/aprove-developers/aprove-releases/releases/download/master_2026_02_15/aprove.jar
cat >/opt/provekit-tools/bin/aprove <<'SH'
#!/usr/bin/env sh
exec java -jar /opt/provekit-tools/aprove-master_2026_02_15/aprove.jar "$@"
SH
chmod +x /opt/provekit-tools/bin/aprove
/opt/provekit-tools/bin/aprove -h >/tmp/aprove-help.txt || true
```

Install CeTA 2.46:

```sh
set -eux
install -d /opt/provekit-tools/ceta-2.46
curl -fL \
  -o /tmp/ceta-2.46-linux-x86_64.tar.gz \
  https://cl-informatik.uibk.ac.at/software/ceta/downloads/ceta-2.46-linux-x86_64.tar.gz
tar -xzf /tmp/ceta-2.46-linux-x86_64.tar.gz -C /opt/provekit-tools/ceta-2.46 --strip-components=1
ln -sf /opt/provekit-tools/ceta-2.46/ceta /opt/provekit-tools/bin/ceta
/opt/provekit-tools/bin/ceta --help >/tmp/ceta-help.txt || true
```

Install CSI 1.2.7:

```sh
set -eux
install -d /opt/provekit-tools/csi-1.2.7
curl -fL \
  -o /tmp/csi-1.2.7-linux-x86_64.tar.gz \
  https://cl-informatik.uibk.ac.at/software/csi/downloads/csi-1.2.7-linux-x86_64.tar.gz
tar -xzf /tmp/csi-1.2.7-linux-x86_64.tar.gz -C /opt/provekit-tools/csi-1.2.7 --strip-components=1
ln -sf /opt/provekit-tools/csi-1.2.7/csi /opt/provekit-tools/bin/csi
/opt/provekit-tools/bin/csi --help >/tmp/csi-help.txt || true
```

Install Wanda 0.6.1 as an alternate termination prover:

```sh
set -eux
install -d /opt/provekit-tools/wanda-0.6.1
curl -fL \
  -o /tmp/wanda-0.6.1-linux-x86_64.tar.gz \
  https://github.com/hezzel/wanda/releases/download/v0.6.1/wanda-0.6.1-linux-x86_64.tar.gz
tar -xzf /tmp/wanda-0.6.1-linux-x86_64.tar.gz -C /opt/provekit-tools/wanda-0.6.1 --strip-components=1
ln -sf /opt/provekit-tools/wanda-0.6.1/wanda /opt/provekit-tools/bin/wanda
/opt/provekit-tools/bin/wanda --help >/tmp/wanda-help.txt || true
```

Install TTT2 1.26 as an alternate termination prover:

```sh
set -eux
install -d /opt/provekit-tools/ttt2-1.26
curl -fL \
  -o /tmp/ttt2-1.26-linux-x86_64.tar.gz \
  https://cl-informatik.uibk.ac.at/software/ttt2/downloads/ttt2-1.26-linux-x86_64.tar.gz
tar -xzf /tmp/ttt2-1.26-linux-x86_64.tar.gz -C /opt/provekit-tools/ttt2-1.26 --strip-components=1
ln -sf /opt/provekit-tools/ttt2-1.26/ttt2 /opt/provekit-tools/bin/ttt2
/opt/provekit-tools/bin/ttt2 --help >/tmp/ttt2-help.txt || true
```

Expose the tools:

```sh
set -eux
echo 'export PATH=/opt/provekit-tools/bin:$PATH' >/etc/profile.d/provekit-portfolio-tools.sh
export PATH=/opt/provekit-tools/bin:$PATH
maude --version
aprove -h >/tmp/aprove-help.txt || true
ceta --help >/tmp/ceta-help.txt || true
csi --help >/tmp/csi-help.txt || true
wanda --help >/tmp/wanda-help.txt || true
ttt2 --help >/tmp/ttt2-help.txt || true
```

Recommended ProvekIt solver config:

```toml
[solvers.maude]
binary = "/opt/provekit-tools/bin/maude"
ir_compiler = "maude"
timeout_seconds = 30
version = "3.5.1"
ceta_gate = true
ceta_binary = "/opt/provekit-tools/bin/ceta"
termination_prover = "/opt/provekit-tools/bin/aprove"
confluence_checker = "/opt/provekit-tools/bin/csi"
```
