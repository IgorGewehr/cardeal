# Fontes originais

Os TTF completos (Inter 4, JetBrains Mono 2, licença OFL — ver `../Inter-LICENSE.txt` e
`../JetBrainsMono-OFL.txt`). **Não são embutidos**: o `cardeal-ui` embute os recortes da pasta
acima, gerados por `scripts/recortar-fontes.sh` (latim estendido A, pontuação, moeda, setas,
sinais, geométricas). Recorte: ~1,75 MB → ~256 KB.

Para regerar depois de trocar uma fonte aqui: `PYFTSUBSET=<caminho> scripts/recortar-fontes.sh`.
