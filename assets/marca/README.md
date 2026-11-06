# Marca

## Lunno (o produto deste ramo)

Uma lua crescente com uma estrela de quatro pontas, em branco sobre o quadrado arredondado na
cor Lua 500 (`#5B45E0`, a escala de marca do design system `cardeal-ui` neste ramo). É uma
marca provisória, desenhada em 2026-11-06 para a primeira demonstração: troque pela
definitiva quando o lunno tiver identidade visual.

- `lunno-icone.svg` — a marca isolada, fonte de verdade.
- `lunno-icone-256.png` / `lunno-icone-512.png` — rasterizações usadas pelo ícone da janela e
  pela barra de título (`cardeal-desktop/src/main.rs`, `LOGO_PNG`, via `include_bytes!`).
- `lunno-logo-horizontal.png` — a marca ao lado do nome, para material de divulgação.
- `gerar_lunno.py` — gera os três PNGs com Pillow (mesma geometria do SVG):
  `python gerar_lunno.py <pasta-destino> <caminho/Inter-SemiBold.ttf>`.

### Cores

- Fundo/marca: `#5B45E0` — `Rubro::R500` do design system (o nome do tipo ficou do Cardeal).
- Branco: `#FFFFFF` — lua e estrela.

## Cardeal (a marca original)

Um cardeal (a ave) geométrico, na cor `#EF443B` — crista, corpo e cauda em formas simples,
pensado pra continuar legível em tamanho pequeno (ícone de janela, favicon, `.desktop`).

- `cardeal-icone.svg` — a marca isolada (fundo quadrado arredondado + ave), fonte de verdade.
- `cardeal-icone-256.png` / `cardeal-icone-512.png` — rasterizações da marca isolada, usadas
  pelo launcher `.desktop` (`~/.local/share/icons/hicolor/256x256/apps/cardeal.png`).
- `cardeal-logo-horizontal.svg` / `.png` — a marca ao lado do nome "Cardeal".

Cores: vermelho `#EF443B`, branco `#FFFFFF` (silhueta da ave), escuro `#2A2A2E` (bico).

## Reexportando em outro tamanho

```bash
magick -background none lunno-icone.svg -resize 128x128 lunno-icone-128.png
```
