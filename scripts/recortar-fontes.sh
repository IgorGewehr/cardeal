#!/usr/bin/env bash
# Recorta as fontes embutidas pelo cardeal-ui para o que a interface usa: português (latim
# estendido A), pontuação tipográfica, moeda, setas, sinais e os poucos símbolos geométricos
# das telas. Sem hinting (o rasterizador do egui não usa) e sem tabelas de layout complexas.
#
# Por quê: os TTF completos (~1,8 MB, com cirílico/grego/vietnamita) entravam inteiros no
# binário do desktop, na RAM e — principalmente — no download do cliente web (ADR-0016).
#
#   python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools
#   PYFTSUBSET=/tmp/ft/bin/pyftsubset scripts/recortar-fontes.sh
set -euo pipefail
cd "$(dirname "$0")/../crates/cardeal-ui/assets/fontes"
PYFTSUBSET="${PYFTSUBSET:-pyftsubset}"

UNICODES="U+0020-007E,U+00A0-00FF,U+0100-017F,U+0192,U+02C6,U+02DA,U+02DC,\
U+2000-206F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,U+25A0-25FF,U+2700-27BF,U+FFFD"

for f in Inter-Regular Inter-Medium Inter-SemiBold JetBrainsMono-Regular JetBrainsMono-Medium; do
  "$PYFTSUBSET" "originais/$f.ttf" \
    --unicodes="$UNICODES" \
    --layout-features='kern' \
    --no-hinting --desubroutinize \
    --name-IDs='0,1,2,3,4,5,6,13,14' \
    --output-file="$f.ttf"
  printf '%-24s %6d KB -> %5d KB\n' "$f" \
    $(( $(stat -c %s "originais/$f.ttf") / 1024 )) $(( $(stat -c %s "$f.ttf") / 1024 ))
done
