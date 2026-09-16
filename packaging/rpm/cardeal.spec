#
# Pacote RPM do Cardeal — binário pré-compilado (não recompila em %build: o binário release
# já foi gerado por `cargo build --release` antes de `rpmbuild` rodar; ver
# `xtask/src/main.rs::empacotar_linux_rpm` e `docs/build/empacotamento.md` para o porquê de
# empacotar binário pronto em vez de ensinar o RPM a chamar `cargo`).
#
# Variáveis esperadas via --define (o xtask já passa todas):
#   app_version  - a versao do workspace (Cargo.toml raiz).
#   have_icon    - 1 se o sourcedir tem cardeal.png, 0 caso contrario (a logo e integrada
#                  por outra sessao de trabalho - este spec nao assume que ela sempre existe).

Name:           cardeal
Version:        %{app_version}
Release:        1%{?dist}
Summary:        ERP para assistência técnica — OS, estoque, vendas, financeiro e PDV
License:        Apache-2.0
URL:            https://github.com/IgorGewehr/cardeal
BuildArch:      x86_64
# O binário é Rust puro estático quanto a libs gráficas (eframe/wgpu + backend X11/Wayland
# via crates puras, sem GTK/Qt do sistema) — só depende da libc/libgcc que todo Fedora
# Workstation já tem por definição. Ver docs/build/empacotamento.md.
Requires:       glibc

%description
ERP desktop para assistência técnica, escrito em Rust puro (egui/eframe), com banco SQLite
próprio. Roda monoposto (um computador, um arquivo de banco) ou conectado a um
`cardeal-server` rodando em outra máquina da rede, para múltiplos computadores acessarem o
mesmo banco por IP. Ver https://github.com/IgorGewehr/cardeal.

%install
rm -rf %{buildroot}
install -Dm755 %{_sourcedir}/cardeal-desktop %{buildroot}%{_bindir}/cardeal-desktop
install -Dm755 %{_sourcedir}/cardeal-server %{buildroot}%{_bindir}/cardeal-server
install -Dm644 %{_sourcedir}/cardeal.desktop %{buildroot}%{_datadir}/applications/cardeal.desktop
%if 0%{?have_icon}
install -Dm644 %{_sourcedir}/cardeal.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/cardeal.png
%endif

%files
%{_bindir}/cardeal-desktop
%{_bindir}/cardeal-server
%{_datadir}/applications/cardeal.desktop
%if 0%{?have_icon}
%{_datadir}/icons/hicolor/256x256/apps/cardeal.png
%endif

%post
update-desktop-database %{_datadir}/applications &> /dev/null || :
gtk-update-icon-cache %{_datadir}/icons/hicolor &> /dev/null || :

%postun
update-desktop-database %{_datadir}/applications &> /dev/null || :
gtk-update-icon-cache %{_datadir}/icons/hicolor &> /dev/null || :

%changelog
* Fri Sep 11 2026 Equipe Cardeal <noreply@cardeal.local> - %{app_version}-1
- Empacotamento RPM inicial (cardeal-desktop + cardeal-server).
