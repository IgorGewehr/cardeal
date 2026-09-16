# Empacotamento Linux — instalador do Cardeal

> Complementa `docs/02-pilar-eficiencia.md` (o binário já é leve e sem dependência gráfica de
> sistema — este documento é só sobre como entregá-lo pronto para uso). Ver também
> `docs/build/atualizacao.md` (o autoatualizador) e `docs/build/rede.md` (servidor/rede).

## 1. O que o binário já garante, de graça

`cargo build --release --bin cardeal-desktop` produz um executável de ~22 MB que só depende
de `libc`/`libm`/`libgcc_s` — confirmado com `ldd` nesta sessão, na máquina real do usuário
(Fedora):

```
linux-vdso.so.1
libgcc_s.so.1 => /lib64/libgcc_s.so.1
libm.so.6     => /lib64/libm.so.6
libc.so.6     => /lib64/libc.so.6
/lib64/ld-linux-x86-64.so.2
```

Sem GTK, sem Qt, sem X11/Wayland do sistema — `eframe`/`wgpu` trazem tudo isso como crates
Rust (features `x11`/`wayland` do `eframe`, ver o Cargo.toml raiz). Isso é o que torna
**qualquer** formato de empacotamento Linux simples: não há "dependências do sistema" reais
para declarar além da libc que toda distro tem.

## 2. Formato escolhido: `.rpm`, não AppImage

O pedido original cotava AppImage como "provavelmente o caminho mais simples e portátil",
com `.rpm` como alternativa válida. Depois de avaliar as duas nesta máquina real (Fedora),
**a decisão foi `.rpm` como formato primário**, pelos motivos abaixo — documentados porque
vão contra a expectativa inicial e alguém revisando isso no futuro merece saber o porquê.

| Critério | `.rpm` (escolhido) | AppImage |
|---|---|---|
| Ferramenta de build | `rpmbuild` — **já vem instalado** no Fedora usado pelo usuário | `appimagetool` — binário de terceiros, precisa baixar da internet na primeira vez |
| Integração com o sistema | `.desktop`/ícone/binário instalados via `dnf`, aparecem no menu, `dnf remove` desinstala limpo, `dnf upgrade` atualiza | precisa integração manual (`.desktop` copiado à mão, ou uma ferramenta externa tipo AppImageLauncher) |
| Dependência em tempo de execução | nenhuma além da libc | **FUSE** — e Fedora Workstation, especificamente, é conhecido por não trazer `libfuse.so.2` por padrão em instalações recentes; é o erro mais comum reportado por usuários de AppImage em Fedora (`dlopen(): error loading libfuse.so.2`) |
| Robustez de manter | um `.spec` versionado, sem elo externo | depende de um binário de terceiros continuar disponível no mesmo URL |
| Portabilidade entre distros | só distros baseadas em RPM (Fedora, RHEL, openSUSE...) | qualquer distro Linux com FUSE funcionando |

Ou seja: para **esta máquina, hoje**, que é a prioridade real do pedido ("é a máquina que o
usuário está usando"), o `.rpm` é estritamente mais robusto — zero download externo, zero
risco de FUSE faltando, integração de sistema de verdade. AppImage continua sendo a escolha
certa se/quando o Cardeal precisar rodar em máquinas de clientes com distros variadas
(Ubuntu, Mint etc.) — por isso o suporte a AppImage foi implementado também (ver §4), só não
como padrão.

## 3. `cargo xtask empacotar-linux`

Comando reproduzível, sem exigir `cargo build` manual de quem for gerar o pacote:

```bash
cargo xtask empacotar-linux                  # .rpm (padrão) — builda release + empacota
cargo xtask empacotar-linux --pular-build    # reusa target/release/ já compilado
cargo xtask empacotar-linux --formato appimage
```

O que o comando `rpm` faz (`xtask/src/main.rs::empacotar_linux_rpm`):

1. `cargo build --release --bin cardeal-desktop --bin cardeal-server` (a menos que
   `--pular-build`).
2. Monta uma árvore `rpmbuild` isolada em `target/rpmbuild/` (nunca em `~/rpmbuild`, para não
   sujar a máquina de quem builda).
3. Copia os dois binários + `packaging/linux/cardeal.desktop` para `SOURCES/`.
4. Procura um ícone em `assets/marca/cardeal-icone-256.png` (ou `-512.png`, ou
   `packaging/linux/icon.png`) — **não cria ícone nenhum** (a logo é trabalho de outra sessão,
   ver `docs/19-estado-e-processo.md`); se não encontrar, empacota sem ícone customizado e
   avisa, em vez de falhar.
5. Roda `rpmbuild -bb packaging/rpm/cardeal.spec` com a versão do workspace
   (`Cargo.toml` raiz, `[workspace.package].version`).
6. Copia o `.rpm` gerado para `dist/` (fora de `target/`, para ficar óbvio onde pegar o
   artefato final — `dist/` está no `.gitignore`).

**Testado de verdade nesta sessão** (não só "deveria funcionar"):

```
$ cargo xtask empacotar-linux --pular-build
...
Pronto: .../dist/cardeal-0.1.0-1.fc44.x86_64.rpm
```

Confirmado com `rpm -qip`/`rpm -qlp`/`rpm -qRp` (metadados corretos, os três arquivos
esperados — `cardeal-desktop`, `cardeal-server`, `cardeal.desktop` — e as dependências de
biblioteca detectadas automaticamente batendo com o `ldd` de §1), e com extração via
`rpm2cpio | cpio` seguida de execução direta do binário extraído num display real desta
máquina (`DISPLAY=:0`), que subiu e rodou por vários segundos sem erro. **Não foi feito**
`sudo dnf install` de verdade nesta sessão — instalar um pacote no gerenciador do sistema é
uma mudança persistente na máquina do usuário que não fazia parte do pedido testar
autonomamente; o próprio usuário deve rodar `sudo dnf install dist/*.rpm` quando quiser
instalar de fato (o pacote está pronto e verificado para isso).

O `packaging/rpm/cardeal.spec` empacota binário **pré-compilado** (não ensina o RPM a chamar
`cargo` em `%build`) — mais simples de manter e mais rápido de gerar, ao custo de não ser um
"source RPM" reconstruível só com `rpmbuild` (quem quiser isso teria que adaptar o spec para
rodar `cargo build` dentro de `%build`, algo para uma rodada futura se fizer falta).

## 4. AppImage (`--formato appimage`) — implementado, não testado de ponta a ponta

O comando monta um `AppDir` (`usr/bin/cardeal-desktop`, `usr/bin/cardeal-server`,
`cardeal.desktop`, `cardeal.png`, `AppRun`) e chama `appimagetool` (baixado uma vez para
`target/tools/appimagetool` se não estiver no `PATH` — precisa de rede só nessa primeira
vez). **Recusa deliberadamente** gerar o pacote se não encontrar um ícone real em
`assets/marca/` — um AppImage sem ícone é uma experiência ruim, e inventar um ícone
violaria a instrução explícita desta sessão de não mexer na identidade visual.

Não foi exercitado de ponta a ponta nesta sessão (a branch de trabalho ainda não tem a logo
mesclada — ver §5 de `docs/19-estado-e-processo.md`); o `.rpm` foi o caminho testado de
verdade. Quando a logo estiver na branch, `cargo xtask empacotar-linux --formato appimage`
deve funcionar sem mudança de código — é o próximo passo de verificação, não uma lacuna de
design.

## 5. Windows — viabilidade pesquisada, cross-compile não tentado nesta rodada

Pesquisa real feita nesta sessão, na própria máquina:

- `rustup target add x86_64-pc-windows-gnu` — funciona, baixa o `rust-std` normalmente.
- O bloqueio real: **não há toolchain C para Windows nesta máquina**
  (`x86_64-w64-mingw32-gcc` ausente). Isso importa porque `rusqlite` está com a feature
  `bundled` (compila o próprio SQLite a partir do C, ver Cargo.toml raiz) — cross-compilar
  para Windows exige um compilador C cruzado funcionando, não só o target Rust.
- Confirmado que o pacote existe nos repositórios Fedora
  (`dnf list mingw64-gcc mingw64-winpthreads-static` — ambos disponíveis), mas **instalar
  pacotes de sistema é uma mudança persistente na máquina do usuário** que esta sessão não
  tomou por conta própria, por ser fora do escopo autônomo esperado (compare com a decisão
  de não rodar `sudo dnf install` do `.rpm` gerado, §3).

**Para retomar** (próxima rodada): `sudo dnf install mingw64-gcc mingw64-winpthreads-static`,
depois `rustup target add x86_64-pc-windows-gnu` e
`cargo build --release --target x86_64-pc-windows-gnu --bin cardeal-desktop`. Os dois pontos
de atenção mais prováveis depois disso, ainda não verificados: (a) `wgpu`/`eframe` com
backend Windows (Direct3D12/Vulkan) — a maioria dos exemplos `eframe` cross-compilam de
Linux para Windows sem drama, mas não foi testado aqui; (b) `rfd` (diálogo de arquivo nativo)
tem backend Windows próprio via `windows-sys`, também não exercitado. Se ambos compilarem
limpo, um instalador `.msi` simples via `cargo-wix` (gera WiX a partir de metadados do
`Cargo.toml`, sem exigir escrever XML na mão) é o caminho mais rápido depois disso — mais
rápido de montar que Inno Setup/NSIS para quem já tem o binário compilando.
