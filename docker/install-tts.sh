#!/bin/sh
set -eu
# Piper engine: GPL-3.0; voice terms are retained alongside each optional model.
apt-get update
apt-get install -y --no-install-recommends python3-venv
python3 -m venv /opt/teamhub-tts
/opt/teamhub-tts/bin/pip install --no-cache-dir 'piper-tts==1.8.0'
mkdir -p /opt/teamhub-voices
base=https://huggingface.co/rhasspy/piper-voices/resolve/375a0fe641dea077c2a47b4e9a056d6da521eed3
download() {
  relative=$1
  digest=$2
  target="/opt/teamhub-voices/$(basename "$relative")"
  curl -fSL --retry 3 --connect-timeout 15 --max-time 600 "$base/$relative" -o "$target"
  printf '%s  %s\n' "$digest" "$target" | sha256sum -c -
}
download tr/tr_TR/dfki/medium/tr_TR-dfki-medium.onnx 2844717f524ab965d3fe86e60562cbb601d3e456836efcc2196cc3a14112a8fb
download tr/tr_TR/dfki/medium/tr_TR-dfki-medium.onnx.json 13ebd7810f1b61b5027583cf3131a0a233b6ea81c38f2200ebc4ff41c3cca039
download en/en_US/lessac/medium/en_US-lessac-medium.onnx 5efe09e69902187827af646e1a6e9d269dee769f9877d17b16b1b46eeaaf019f
download en/en_US/lessac/medium/en_US-lessac-medium.onnx.json efe19c417bed055f2d69908248c6ba650fa135bc868b0e6abb3da181dab690a0
curl -fSL "$base/tr/tr_TR/dfki/medium/MODEL_CARD" -o /opt/teamhub-voices/tr-MODEL_CARD
curl -fSL "$base/en/en_US/lessac/medium/MODEL_CARD" -o /opt/teamhub-voices/en-MODEL_CARD
apt-get clean
rm -rf /var/lib/apt/lists/*
