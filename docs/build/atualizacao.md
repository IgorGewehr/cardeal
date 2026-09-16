# Autoatualização — `cardeal_cliente::atualizador`

> Complementa `docs/build/empacotamento.md` (como o binário/pacote é gerado) e
> `.github/workflows/release.yml` (como a release chega ao GitHub, de onde este módulo lê).

## 1. Mecanismo

Sem infraestrutura própria de servidor de update: `verificar_atualizacao` consulta
`GET https://api.github.com/repos/IgorGewehr/cardeal/releases/latest` diretamente. Um 404
(repositório sem nenhuma release ainda, ou release removida) é tratado como "nada para
atualizar" (`Ok(None)`), não como erro — um build local rodando contra o repositório antes da
primeira release nunca deve incomodar o usuário com um aviso de falha.

A comparação de versão é semver simples (`versao_semver`, aceita prefixo `v` e ignora sufixo
tipo `-rc1`); qualquer tag que não seja reconhecível como semver é tratada como "não dá para
comparar, não ofereça a atualização", nunca como falso positivo.

## 2. Convenção de asset

Além dos pacotes de instalação (`.rpm`/`.AppImage`, que não se autoaplicam — precisam do
gerenciador de pacotes do sistema), toda release deve anexar um binário **puro** chamado
exatamente `cardeal-desktop-linux-x86_64`. É nele que `verificar_atualizacao` procura
(`ASSET_LINUX` em `atualizador.rs`) e é o que `baixar_atualizacao`/`aplicar_atualizacao`
sabem substituir no lugar do executável em execução. `.github/workflows/release.yml` gera
esse asset copiando `target/release/cardeal-desktop` para `dist/cardeal-desktop-linux-x86_64`
antes de publicar.

Uma release sem esse asset ainda é reportada ao usuário (`VersaoDisponivel.versao`/`notas`),
só sem o botão de aplicar direto — `url_binario_linux` fica `None`.

## 3. Aplicar a atualização exige que o executável atual seja gravável

Um Cardeal instalado via `.rpm` mora em `/usr/bin`, que só root escreve. `aplicar_atualizacao`
detecta isso **antes de baixar** (abre o executável atual para escrita sem truncar) e devolve
`ResultadoAplicacao::RequerPermissaoDoSistema` com o caminho, para a UI orientar
`sudo dnf upgrade cardeal` em vez de tentar e falhar tarde ou escalar privilégio sozinho — este
projeto nunca faz isso. Um Cardeal rodando de `target/release/` (desenvolvimento) ou de um
layout portátil é gravável e se autoatualiza de verdade: grava num arquivo temporário ao lado
e usa `rename` (atômico — o processo em execução mantém o inode antigo aberto; a próxima
execução usa o novo).

## 4. UI (`cardeal-desktop`)

`App::novo` dispara uma verificação em background ao abrir (thread separada — é uma chamada
HTTP bloqueante, não pode travar o quadro do `egui`). A paleta de comandos (Ctrl+K) expõe
"Verificar atualizações" e "Baixar e instalar atualização" a qualquer momento. Encontrar uma
versão nova gera um toast informativo; aplicar com sucesso relança o processo no binário novo
(`reiniciar_no_novo_binario`, sem rodar destrutores — seguro porque o WAL do SQLite sobrevive
a encerramento abrupto, `docs/03-pilar-resiliencia.md`); `RequerPermissaoDoSistema` vira um
toast de aviso com a instrução de `sudo dnf upgrade`.

## 5. O que falta

- Só Linux — `.msi`/Windows depende do cross-compile ainda não tentado (ver
  `docs/build/empacotamento.md` §5).
- Sem assinatura/checksum do binário baixado — a release do GitHub já serve por HTTPS, mas
  não há verificação adicional (ex.: hash publicado na release, comparado antes de aplicar).
  Considerar antes de distribuir a clientes fora da máquina de desenvolvimento.
