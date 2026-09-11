# Marca do Cardeal

Um cardeal (a ave) geométrico, na cor Rubro 500 (`#EF443B`, o tom que já era a identidade
visual do design system `cardeal-ui`) — crista, corpo e cauda em formas simples, pensado pra
continuar legível em tamanho pequeno (ícone de janela, favicon, `.desktop`).

## Arquivos

- `cardeal-icone.svg` — a marca isolada (fundo quadrado arredondado + ave), fonte de verdade.
- `cardeal-icone-256.png` / `cardeal-icone-512.png` — rasterizações da marca isolada, usadas
  pelo ícone da janela (`cardeal-desktop/src/main.rs`, `icone_janela()`, via `include_bytes!`)
  e pelo launcher `.desktop` (`~/.local/share/icons/hicolor/256x256/apps/cardeal.png`).
- `cardeal-logo-horizontal.svg` / `.png` — a marca ao lado do nome "Cardeal", para documentação,
  splash screen ou material de divulgação.

## Cores

- Vermelho (fundo/marca): `#EF443B` — `Rubro::R500` do design system.
- Branco: `#FFFFFF` — silhueta da ave.
- Escuro (bico): `#2A2A2E`.

## Reexportando em outro tamanho

```bash
magick -background none cardeal-icone.svg -resize 128x128 cardeal-icone-128.png
```
