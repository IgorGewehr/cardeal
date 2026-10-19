# Módulo `fiscal`

> Estado: **onda 7, fatias 1 e 2** — configuração (certificado A1, dados fiscais do emitente, ambiente,
> conexão), a **NFC-e do PDV** como documento assíncrono (fila, transmissão em segundo plano, conferência
> pela chave), o **DANFCE** (cupom de 80 mm com QR Code), o **cancelamento** e a **inutilização**. Faltam
> contingência offline, série por terminal, NF-e, NFS-e e SPED (§11).
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

Conferido em 2026-10-14 contra `emissao.tensorroot.com` com o A1 da Tensorroot (regravado): SEFAZ-SP (a UF
da Tensorroot) e SEFAZ-SC em homologação responderam **107 — Serviço em Operação** para NF-e e NFC-e
(`cardeal-fiscal`, teste ignorado `homologacao_status_real`, com `CARDEAL_TESTE_UF`).

**Numeração já usada fora do Cardeal:** uma empresa que já emitiu por outro sistema (a Tensorroot, pelo
`saas-erp`: NFC-e e NF-e série 1 até o nº 2, NFS-e até o nº 5, em produção) não pode recomeçar do 1 na mesma
série — a SEFAZ rejeita por duplicidade. A numeração do Cardeal (próxima fatia) começa de um número inicial
informado na configuração, por série.

## 6. Recursos injetados

`ProvedorFiscal` e `Cofre` chegam pelos `cardeal_modkit::Recursos` (`Ctx::recurso::<T>()`), montados
por `cardeal_distribuicao::recursos(pasta)` no desktop e no servidor:

- `ProvedorFiscal::sefaz_api(config)` — com `CARDEAL_FISCAL_TOKEN`;
- `ProvedorFiscal::nao_configurado()` — sem a chave: toda conversa com a SEFAZ responde "a API fiscal não
  está configurada nesta instalação";
- `ProvedorFiscal::simulado(porta)` — testes e demonstração (`recursos_simulados`).

`mod_fiscal::porta_da_empresa(ctx, conexão)` abre o certificado do cofre e devolve a `PortaFiscal` da
empresa no ambiente configurado — é por ela que a emissão vai passar.

## 7. NFC-e das vendas (fatia 2)

**Configuração** (`ConfigurarNfce`, `fiscal.configurar_nfce.v1`): emitir a cada venda do PDV, série (1 a
889), próximo número (por série **e ambiente**; nunca abaixo do maior já usado) e o CSC — o token vai para o
cofre (contexto `fiscal.csc/<empresa>`) e nunca volta para a tela.

**O documento nasce na venda, sem rede** (`EmitirNfceDoCupom`, `fiscal.emitir_nfce_do_cupom.v1`, permissão
nova `fiscal.emitir`): reserva o número, monta a nota a partir do cupom finalizado — itens com o produto e o
**perfil fiscal efetivo** (`mod_estoque::produtos_para_nota`), pagamentos, o CPF/CNPJ do cliente identificado
— e a guarda (postcard, sem segredo) em `fiscal_documento`. Faltando algo (produto sem NCM, perfil com CST
do regime errado, certificado/CSC/dados fiscais), o documento já nasce **rejeitado** com a frase do que
corrigir. Repetir devolve o mesmo documento. `ReenviarDocumento` remonta uma rejeitada com o **mesmo número**
(a rejeição não o consome). Em homologação, o primeiro item sai com a descrição que a SEFAZ exige ("NOTA
FISCAL EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL").

**Tributação** (`tributar`, pura): Simples — CSOSN 102/103/300/400/500 (sem perfil, 102), PIS/COFINS com
valores zerados; regime normal — CST 00, 20 (base reduzida), 40, 41, 50, 60, PIS/COFINS 01/02 calculados.
CFOP do perfil, ou 5405 para ICMS retido e 5102 para o resto; fora do 5xxx é recusado (NFC-e é interna).

**A transmissão é uma tarefa** (`fiscal.transmitir_pendentes`, `cardeal_modkit::Registro::tarefa`): roda no
processo do motor, fora de qualquer transação — lê a fila, abre certificado e CSC do cofre, fala com a API
documento a documento e grava cada resultado na hora. No desktop ela roda numa thread (`ExecutorDeTarefas`):
depois de cada venda e, a cada minuto, se houver nota esperando. A decisão de cada resposta é uma função
pura (`decidir`):

| Situação | Vai para |
|---|---|
| Autorizada (100/150) | `Autorizada` — chave, protocolo, XML |
| Rejeitada pela SEFAZ (inclui as de código < 200, ex.: SP 178) | `Rejeitada` — corrigir e reenviar |
| Denegada | `Denegada` (o número foi consumido) |
| Resposta sem resultado (timeout, erro com chave) | `Incerta` — **consultar pela chave** antes de qualquer reemissão |
| Duplicidade 539 (a chave da nota que existe vem no motivo) / 204 | `Incerta` — conferir essa chave |
| Consulta: não consta (217) | `Pendente` — emitir de novo, já |
| Consulta: 101/151/155 | `Cancelada` (o `sefaz-api` diz "autorizado" para o 101) |
| API fora do ar / SEFAZ paralisada (108/109) | continua, espera 30 s → 1, 2, 4, 8 → 15 min |
| Recusa do provedor/instalação (CNPJ, credencial, cofre) | `Rejeitada` com a mensagem |
| Erro do emissor repetido 6 vezes | `Rejeitada` — não gira na fila para sempre |

Cada documento vai ao **seu** ambiente (a nota de homologação que ficou na fila não sai em produção). Duas
transmissões ao mesmo tempo para a mesma empresa não acontecem (trava por processo). O `cStat` vira frase
de balcão (`explicar`: o que houve e onde corrigir).

**Conferido de verdade em 2026-10-15** com o `sefaz-api` local (o publicado estava fora do ar — Cloudflare
1033): a NFC-e montada pelo Cardeal passou por assinatura, schema e QR Code na SEFAZ-SP de homologação e foi
recusada só por cadastro (178, a Tensorroot saiu de SP) — o que levou à regra das rejeições com código
< 200. Falta a primeira **autorizada**: pede a IE do RS e o CSC de homologação da SEFAZ-RS
(`cardeal-fiscal`, teste ignorado `homologacao_nfce_real`).

**Cancelamento e inutilização são pedidos na fila** (migração v3): `CancelarNfce`
(`fiscal.cancelar_nfce.v1`) numa autorizada e `InutilizarNumero` (`fiscal.inutilizar_numero.v1`) numa
rejeitada que não vai mais sair (o número não vira buraco na série) — permissão nova `fiscal.cancelar`,
justificativa de 15 a 255 caracteres. O comando só registra o pedido; a mesma tarefa da fila o leva à SEFAZ
(sem internet, ele espera). `decidir_evento`: aceito (135/136/155, 102) → `Cancelada`/`Inutilizada`; já feito
antes (573/218, 563/206/256) → idem; recusado (ex.: 501 fora do prazo) → a nota volta ao que era, sem o pedido,
com o motivo; sem resposta → o pedido espera. O cancelamento manual **não desfaz a venda**: para desfazer a venda,
use o estorno do PDV (`F11`), que anula a nota junto.

**Venda estornada no PDV** (migração v4, coluna `anulada`): `AnularDocumentoDaVenda`
(`fiscal.anular_documento_da_venda.v1`, permissão `fiscal.emitir` — é consequência do estorno, que
já exigiu supervisor; o comando recusa venda que não esteja `Estornado`) marca o documento **de
forma permanente** e pede o que o estado pede (`pedido_da_anulacao`):
- autorizada → cancelar;
- rejeitada → inutilizar (e não se reenvia mais);
- pendente **nunca transmitida** → inutilizar o número, sem emitir;
- pendente já tentada ou incerta → nada ainda: a tentativa pode ter chegado à SEFAZ sem resposta, então
  a fila emite (a duplicidade revela a chave) ou confere, e o documento que chega a autorizado ou
  rejeitado já recebe o pedido na mesma escrita. O prazo de 30 minutos conta da autorização, então o
  cancelamento logo depois cabe;
- denegada/cancelada/inutilizada → nada.

Se a SEFAZ recusar o cancelamento (fora do prazo), a nota continua autorizada e
`DocumentoResumo::anulacao_pendente` a mostra como "cancelamento recusado" (no PDV e em Notas
fiscais), com a orientação de emitir uma NFC-e de devolução; a fila não insiste. A venda estornada
antes de a NFC-e existir não gera nota (o `EmitirNfceDoCupom` exige venda `Finalizado`).
`DocumentosDasVendas` (`fiscal.documentos_das_vendas.v1`) devolve as NFC-e de várias vendas numa
consulta, para a lista do dia.

A venda **a prazo** do PDV vai na NFC-e como `tPag 05` (crédito loja).

**DANFCE** (`cardeal_pdf::gerar_danfce`): bobina de 80 ou 58 mm (Configurações › Impressão; no de 58 mm a
fonte e a margem encolhem e o QR Code fica com 28 mm, acima dos 25 mm do manual) com a altura do cupom, no leiaute do manual
(emitente, itens, totais, pagamentos e troco, chave em 11 grupos com o endereço de consulta, consumidor,
número/série/emissão, protocolo e o QR Code — `qrcodegen`, MIT). Em homologação, "sem valor fiscal" no topo e
no rodapé. Montado da nota guardada + `qrCode`/`urlChave` do XML autorizado; abre no visualizador do sistema
(de onde vai à impressora — a impressão direta ESC/POS é passo seguinte). Conferido: o QR Code renderizado
decodifica para a URL de consulta com a chave.

## 8. Telas

Configurações › **Fiscal**: pendências no topo; dados fiscais do emitente; certificado (titular, CNPJ,
validade com etiqueta "válido/vence em N dias/vencido", trocar, remover, aviso quando foi regravado);
ambiente e **Testar conexão** (NF-e e NFC-e, com o código e o motivo da SEFAZ). Na Visão geral, o
certificado vencido ou a 30 dias de vencer vira aviso com atalho para esta aba (empresa sem
certificado não vê aviso — nem toda empresa emite).

O teste de conexão bloqueia a tela até a resposta (o cliente HTTP tem tempo limite). A emissão não
bloqueia: é assíncrona por desenho ([doc 10 §3](../10-modulo-fiscal.md)).

Também em Configurações › Fiscal, o cartão **NFC-e nas vendas do PDV** (emitir ou não, série, próximo
número, CSC). No **PDV**, cada venda finalizada cria a nota e dispara a transmissão em segundo plano; o
cabeçalho mostra a faixa da fila ("Fiscal em dia" / "N na fila" / "N rejeitadas") e o resultado da venda, a
situação da NFC-e (com o que corrigir, se rejeitada). Área nova **Notas fiscais** (barra lateral, grupo
Financeiro): indicadores (na fila, conferindo, rejeitadas, autorizadas hoje), filtro, grade com
**Reenviar** e **Detalhes** (chave, protocolo, itens com NCM/CFOP, consumidor; **Imprimir cupom fiscal**,
**Cancelar NFC-e** e **Inutilizar número** com a justificativa) e **Transmitir agora**. No PDV, a venda
autorizada ganha **Imprimir cupom fiscal**.

## 9. Permissões

| Permissão | Risco | O que libera |
|---|---|---|
| `fiscal.ver` | Baixo | Situação fiscal, notas, teste de conexão |
| `fiscal.emitir` | Médio | Criar a NFC-e da venda, reenviar, transmitir a fila |
| `fiscal.cancelar` | Alto | Cancelar NFC-e autorizada, inutilizar número |

O papel de fábrica **Operador de Caixa** recebe `fiscal.ver` e `fiscal.emitir` (2026-10-15): sem
eles, a venda feita por um operador saía **sem NFC-e**, em silêncio (o PDV só emite com os dois).
Cancelar e configurar continuam fora do papel.
| `fiscal.configurar` | Alto | Certificado, ambiente, configuração da NFC-e (CSC) |

## 10. Tabelas

- v1: `fiscal_certificado` (uma linha por empresa: parte pública, `selado`, quem instalou) e
  `fiscal_configuracao` (o ambiente).
- v2: `fiscal_configuracao` ganha `emitir_nfce`, `serie_nfce`, `csc_id`, `csc_selado`; `fiscal_serie`
  (próximo número por empresa, modelo, série e ambiente); `fiscal_documento` (a nota guardada, estado,
  chave, protocolo, código e motivo, XML, tentativas e próxima tentativa; únicos por origem e por
  série+número+ambiente).
- v3: `fiscal_documento` recriada (o SQLite não muda `CHECK`) com o estado `Inutilizada` e o pedido
  (`pedido`, `justificativa`, `protocolo_evento`, `evento_em`).
- v4: `fiscal_documento.anulada` — a venda foi desfeita (estorno no PDV).

## 11. Próximas fatias da onda 7

1. **Primeira NFC-e autorizada no RS** (falta a IE, o endereço e o CSC de homologação da SEFAZ-RS).
2. A impressão direta na térmica (ESC/POS) e o DANFCE em 58 mm.
3. **Contingência offline** (`tpEmis=9`, `prepare`/`transmit` do `sefaz-api`): exige a API alcançável — sem
   internet nenhuma, a nota espera na fila (assinar localmente seria trazer a assinatura para o Cardeal).
4. Série por terminal (hoje uma série por empresa).
5. NF-e de venda e devolução; CC-e.
6. NFS-e da OS.
7. SPED gerado no Cardeal.
8. DFe e manifestação — dependem de rotas novas no `sefaz-api`.
