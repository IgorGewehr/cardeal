# Módulo `fiscal`

> Estado: **onda 7, fatia 1 (configuração)** — certificado A1, dados fiscais do emitente, ambiente e
> conexão com a SEFAZ. A emissão (NFC-e, NF-e, NFS-e) entra nas próximas fatias, sobre o que está aqui.
> Arquitetura: [ADR-0006](../adr/0006-fiscal-via-api-externa.md) (fiscal por API externa) e
> [ADR-0019](../adr/0019-sefaz-api-como-provedor-e-cofre-do-certificado.md) (o provedor é o `sefaz-api`;
> o certificado mora cifrado no cofre). Visão geral: [doc 10](../10-modulo-fiscal.md).

## 1. Escopo

**Faz:** guarda e confere o certificado digital A1 da empresa; mantém o ambiente
(homologação/produção); conversa com a SEFAZ pelo `sefaz-api`; lista o que falta para emitir.
Nas próximas fatias: o documento fiscal como agregado assíncrono (fila, numeração, contingência),
DANFE/DANFCE e SPED.

**Não faz:** assinatura e transmissão SOAP (são do `sefaz-api`); cálculo de tributos (vem do perfil
tributário do Cardeal — o `sefaz-api` só confere `BC × alíquota`); dados cadastrais da empresa (são
de `mod-empresa`, que agora guarda também os **dados fiscais do emitente**).

## 2. O provedor: `sefaz-api`

O motor fiscal próprio da operação (`IgorGewehr/sefaz-api`, publicado em
`https://emissao.tensorroot.com`). Contrato da branch `main` (a publicada), levantado em 2026-10-14:

| Aspecto | Como é | O que o Cardeal faz com isso |
|---|---|---|
| Autenticação | `Authorization: Bearer <chave>` | `CARDEAL_FISCAL_TOKEN` (e `CARDEAL_FISCAL_URL` para outro servidor) |
| Certificado | `pfxBase64` + `password` **no corpo de toda chamada**; o serviço não guarda nada | O Cardeal guarda, cifrado no cofre, e abre só para a chamada |
| CNPJ | Recusa (403) certificado de CNPJ diferente do emitente — comparação exata, não pela raiz | Conferido já na instalação |
| PFX com RC2 | O TLS dele (Node/OpenSSL 3) recusa: "Unsupported PKCS12 PFX data" | Regravado na instalação (§3) |
| Numeração | O emissor manda o `numero`; o `cNF` é sorteado a cada chamada | Numeração é nossa; **nunca reenviar sem consultar a chave devolvida** (re-emitir o mesmo número gera outra chave → rejeição 539) |
| Cobertura | NF-e, NFC-e (com contingência offline `prepare`/`transmit`), cancelamento, CC-e, inutilização, consulta, status, NFS-e (Nacional/ADN, Betha SC/RS, Paulistana), MDF-e | Porta `PortaFiscal` cresce fatia a fatia |
| Não tem | Distribuição DFe, manifestação, DANFE, SPED, cálculo de tributos | DFe/manifestação: a criar no `sefaz-api` (com OK do dono). DANFE e SPED: no Cardeal |

A branch `feat/nestjs-fiscal-engine` (v2 multi-tenant, guarda certificado, DANFE, SPED) está parada
desde 2026-05-07 e sem as correções que a `main` recebeu depois — o Cardeal fala com a `main`.

## 3. Certificado A1

`InstalarCertificado { arquivo, senha }` (`fiscal.instalar_certificado.v1`, `fiscal.configurar`):

1. `cardeal_fiscal::preparar_certificado_a1` abre o PKCS#12 em Rust puro (PBES2 com AES/3DES,
   PBES1 com 3DES/RC2, MAC SHA-1/2) e confere a senha; lê titular, CNPJ (extensão ICP-Brasil
   `2.16.76.1.3.3`, ou o `:CNPJ` do fim do `CN`), certificadora e validade.
2. Se o arquivo usa **RC2** (exportação antiga — o A1 real da Tensorroot é assim), regrava com a mesma
   senha: certificados em claro (são públicos), chave em 3DES, MAC HMAC-SHA256. É o que o `saas-erp`
   faz em produção. Arquivo já em AES/3DES fica intacto.
3. Recusa: senha errada, vencido, sem CNPJ (e-CPF), CNPJ diferente do da empresa.
4. Sela `(arquivo preparado, senha)` com o `Cofre` da instalação (AES-256-GCM, contexto
   `fiscal.certificado/<empresa>`) e grava em `fiscal_certificado.selado`. A base, os backups e as
   réplicas só têm texto cifrado; a resposta do comando (guardada para idempotência) só tem a parte
   pública.

**O cofre** (`cardeal_storage::Cofre`): a chave vem de `CARDEAL_CHAVE_SEGREDOS` (64 dígitos hex — o
servidor recebe do gerenciador de segredos) ou do arquivo `cofre.chave` na pasta de dados (criado na
primeira abertura; no Unix, `0600`). Quem copia só o `.db` leva texto cifrado; quem leva o disco inteiro
leva a chave junto — proteger a chave com o armazenamento do sistema (DPAPI no Windows) é o próximo passo.
Selado com outra chave (outra instalação, base restaurada em outra máquina) responde "guardado em outra
instalação — envie de novo".

`RemoverCertificado` apaga a linha (o selado vai junto). Instalar outro substitui.

## 4. Dados fiscais do emitente (`mod-empresa`)

`DefinirDadosFiscaisEmpresa` (`empresa.definir_dados_fiscais.v1`, `empresa.dados.editar`) e
`DadosFiscaisDaEmpresa` (`empresa.dados_fiscais.v1`): IE (ou `ISENTO`), IM (NFS-e) e o endereço com
**código IBGE do município** (7 dígitos, os 2 primeiros têm de ser os da UF), CEP de 8 dígitos. Ficam
em `nucleo_empresa` (`ie`, `im`, `endereco` em JSON). O erro volta com o **campo** marcado. Sem
endereço de exibição ainda, a gravação monta um (cabeçalho dos PDFs).

## 5. Ambiente, pendências e conexão

- `DefinirAmbienteFiscal` (`fiscal.definir_ambiente.v1`): homologação é o padrão; produção exige
  certificado instalado.
- `SituacaoFiscal` (`fiscal.situacao.v1`, `fiscal.ver`): certificado (parte pública), ambiente e
  pendências — `SemCertificado`, `CertificadoVencido`, `CertificadoVencendo` (até 30 dias; ainda emite),
  `DadosFiscaisIncompletos` — do que impede ao que só avisa.
- `StatusDaSefaz { modelo }` (`fiscal.status_sefaz.v1`): o status do serviço de autorização da SEFAZ
  da UF da empresa, pelo provedor, com o certificado do cofre — a conferência ponta a ponta (certificado,
  chave da API, rede). É uma consulta que fala com a rede: segura uma conexão de leitura até a resposta.

Conferido em 2026-10-14 contra `emissao.tensorroot.com` com o A1 da Tensorroot (regravado): SEFAZ-SC em
homologação respondeu **107 — Serviço em Operação** para NF-e e NFC-e
(`cardeal-fiscal`, teste ignorado `homologacao_status_real`).

## 6. Recursos injetados

`ProvedorFiscal` e `Cofre` chegam pelos `cardeal_modkit::Recursos` (`Ctx::recurso::<T>()`), montados
por `cardeal_distribuicao::recursos(pasta)` no desktop e no servidor:

- `ProvedorFiscal::sefaz_api(config)` — com `CARDEAL_FISCAL_TOKEN`;
- `ProvedorFiscal::nao_configurado()` — sem a chave: toda conversa com a SEFAZ responde "a API fiscal não
  está configurada nesta instalação";
- `ProvedorFiscal::simulado(porta)` — testes e demonstração (`recursos_simulados`).

`mod_fiscal::porta_da_empresa(ctx, conexão)` abre o certificado do cofre e devolve a `PortaFiscal` da
empresa no ambiente configurado — é por ela que a emissão vai passar.

## 7. Telas

Configurações › **Fiscal**: pendências no topo; dados fiscais do emitente; certificado (titular, CNPJ,
validade com etiqueta "válido/vence em N dias/vencido", trocar, remover, aviso quando foi regravado);
ambiente e **Testar conexão** (NF-e e NFC-e, com o código e o motivo da SEFAZ). Na Visão geral, o
certificado vencido ou a 30 dias de vencer vira aviso com atalho para esta aba (empresa sem
certificado não vê aviso — nem toda empresa emite).

O teste de conexão bloqueia a tela até a resposta (o cliente HTTP tem tempo limite). A emissão não vai
bloquear: é assíncrona por desenho ([doc 10 §3](../10-modulo-fiscal.md)).

## 8. Permissões

| Permissão | Risco | O que libera |
|---|---|---|
| `fiscal.ver` | Baixo | Situação fiscal e teste de conexão |
| `fiscal.configurar` | Alto | Instalar/remover certificado, trocar o ambiente |

## 9. Tabelas (migração `fiscal` v1)

- `fiscal_certificado` — uma linha por empresa: parte pública (titular, CNPJ, certificadora, série,
  validade, `convertido`), `selado` (o cofre), quem instalou e quando.
- `fiscal_configuracao` — o ambiente (`homologacao`/`producao`).

## 10. Próximas fatias da onda 7

1. Documento fiscal assíncrono: numeração e séries por terminal, fila durável, consulta antes de
   reemitir, tradução amigável do `cStat`. Primeiro a **NFC-e do PDV** (CSC por empresa, contingência
   offline com `prepare`/`transmit`).
2. NF-e de venda e devolução; cancelamento, CC-e, inutilização.
3. DANFE e DANFCE em PDF (`cardeal-pdf`), com QR Code.
4. NFS-e da OS.
5. SPED gerado no Cardeal.
6. DFe e manifestação — dependem de rotas novas no `sefaz-api`.
