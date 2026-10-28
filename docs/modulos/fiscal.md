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
| Não tinha | Distribuição DFe e manifestação do destinatário — criadas em 2026-10-26 (842576f: `/nfe/dfe/distribuicao`, `/nfe/dfe/consulta-chave`, `/nfe/manifestacao`) | A busca de notas de compra, em `mod-compras` ([compras.md](compras.md) §5) |
| Não tem | DANFE, SPED, cálculo de tributos | DANFE e SPED: no Cardeal |

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
| Denegada | `Denegada` (o número foi consumido) — com o XML da denegação, que também se guarda |
| **SEFAZ fora e a nota não chegou a ela** (108/109, ou o `naoEnviado` do provedor: circuito aberto, sem conexão, 503) | `Contingencia` — assinada em contingência off-line, já (ver abaixo); se a assinatura falha, continua `Pendente` e espera |
| Resposta sem resultado (timeout, erro com chave) | `Incerta` — **consultar pela chave** antes de qualquer reemissão; a nota assinada que foi à SEFAZ fica guardada (`xml_assinado`) e vai na consulta, que devolve o `nfeProc` |
| Duplicidade 539 (a chave da nota que existe vem no motivo) / 204 | `Incerta` — conferir essa chave |
| Consulta: não consta (217) | `Pendente` — emitir de novo, já (a de contingência, chave com `tpEmis` 9, volta a `Contingencia`: o mesmo XML) |
| Consulta: 101/151/155 | `Cancelada` — com o protocolo da autorização e o XML do cancelamento (o `sefaz-api` dizia "autorizado" para o 101 até bc58e77) |
| API fora do ar (sem quem assine) | continua `Pendente`, espera 30 s → 1, 2, 4, 8 → 15 min |
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

**NF-e de venda** (modelo 55; migração v10; 2026-10-27). `EmitirNfeDoPedido`
(`fiscal.emitir_nfe_do_pedido.v1`, `fiscal.emitir`) cria a NF-e de um pedido de vendas **faturado**, sem rede,
e ela vai pela mesma fila (o `sefaz-api` faz o lote e o polling; `PortaFiscal::emitir_nfe`). Repetir devolve o
mesmo documento; `DocumentoDoPedido` (`fiscal.documento_do_pedido.v1`) acha a nota do pedido; a rejeitada se
reenvia com o mesmo número depois de corrigir o cadastro. A montagem (`montar_nfe`, pura) reaproveita os itens e a
tributação da NFC-e e acrescenta:

- **Destinatário** do cadastro do cliente (`destinatario_do_cliente`): CNPJ antes do CPF; a inscrição estadual
  decide o `indIEDest` (com IE, contribuinte; "ISENTO", isento; sem, não contribuinte — e aí `indFinal = 1`); o
  endereço principal, com o **código IBGE do município achado pelo nome da cidade** (a tabela do IBGE vai
  embutida: `cardeal_kernel::municipio`, 5.571 municípios, sem rede) — o cadastro de clientes não pede o código;
  o e-mail principal.
- **Venda para outra UF**: o CFOP vai para o interestadual (`5xxx` → `6xxx`; `5405` → `6404`). No Simples
  Nacional sai (o ICMS vai no DAS, sem destaque); no **regime normal ainda não** — a alíquota interestadual e o
  DIFAL não estão no cálculo, e a nota nasce rejeitada dizendo isso.
- **Pagamento**: à vista, `indPag = 0`; a prazo (o faturamento lançou o título a receber do pedido), `indPag = 1`
  e as parcelas viram as **duplicatas** (`cobr/dup`, numeradas 001, 002…), que precisam fechar com o total. O
  meio vai como "outros" (`tPag = 99`) com a descrição "À vista"/"A prazo": o pedido não diz se vai ser boleto,
  Pix ou depósito.
- `indPres = 9` (não presencial) e `modFrete = 9` (sem frete) por padrão; natureza "Venda de mercadoria".
- **Simples Nacional**: as informações complementares levam o texto que a Resolução CGSN 140/2018 (art. 59)
  exige ("documento emitido por ME ou EPP optante pelo Simples Nacional; não gera direito a crédito fiscal de
  IPI").

A série e o próximo número da NF-e (`ConfigurarNfe`/`ConfiguracaoDaNfe`) são separados dos da NFC-e. A
contingência off-line e a substituição são só da NFC-e (a contingência da NF-e é outra, a SVC). Desktop: Vendas ›
pedido faturado › **Emitir NF-e** (com a situação e o "Reenviar"); Configurações › Fiscal › NF-e de venda; as
Notas fiscais mostram o modelo. Lição do gestao-raiz: lá a NF-e sai também de um formulário em que se digitam o
destinatário e os itens; aqui só do pedido faturado — o cadastro é a fonte.

**DANFE A4** (`cardeal_pdf::gerar_danfe`, 2026-10-27): o leiaute do manual — canhoto, emitente, o quadro DANFE
(entrada/saída, número, série, folha), o **código de barras da chave em Code 128C** (`cardeal_pdf::barras`, escrito
aqui; conferido com um leitor de verdade, o zxing, que leu a chave exata), natureza e protocolo, destinatário,
duplicatas (ou "pagamento à vista"), cálculo do imposto, transportador, produtos (a descrição quebra em linhas;
o que não cabe segue nas folhas seguintes, com o cabeçalho e "FOLHA n/total") e dados adicionais; em homologação,
"SEM VALOR FISCAL". A data de emissão sai do `dhEmi` do XML autorizado. Imprimir: Notas fiscais › Detalhes e
Vendas › pedido › Imprimir DANFE (o PDF no visualizador).

**Carta de correção** (CC-e, evento 110110; `mod_fiscal::correcao`, migração v11, 2026-10-27):
`EmitirCartaDeCorrecao` (`fiscal.emitir_carta_de_correcao.v1`, `fiscal.emitir`) pede a carta de uma NF-e
**autorizada** (a NFC-e não tem CC-e), com o texto de 15 a 1000 caracteres (os espaços e as quebras de linha
viram um espaço só); a **fila** a leva à SEFAZ (`PortaFiscal::carta_de_correcao`, HTTP `/nfe/carta-correcao`),
como a nota — sem internet, espera, com a mesma espera crescente. Regras:
- uma por vez: com uma carta esperando a fila, não se pede outra;
- até **20** registradas por nota;
- a sequência é a seguinte à **última registrada** (a recusada não a gasta);
- a resposta 573 ("duplicidade de evento" — a primeira tentativa chegou e a resposta se perdeu) conta como
  registrada.

Cada carta substitui a anterior, então o formulário já começa com o texto da última registrada. A tela
explica o que a carta não corrige (valores, quantidades, impostos, CNPJ/CPF, datas — Ajuste SINIEF 01/07).
`CartasDeCorrecao` (`fiscal.cartas_de_correcao.v1`) lista as da nota. O `procEventoNFe` vai ao pacote do
contador em `Saidas/NF-e/CartasDeCorrecao/110110<chave><seq>-procEventoNFe.xml`, no mês do **registro**,
e o relatório conta as cartas. Comparado com o gestao-raiz (`api/fiscal/invoices/[id]/cce`): a mesma
sequência (a maior aceita + 1; lá a recusada nem é guardada), mas a CC-e vai direto, sem fila — sem internet
falha, e a resposta perdida faz a nova tentativa voltar com 573, que lá vira erro mesmo com a carta
registrada; o XML do evento é guardado "se der" (aqui vai na mesma escrita do registro).
Junto, o pacote passou a dar ao cancelamento por substituição o nome com o tipo dele
(`110112<chave>01-procEventoNFe.xml`) — saía como 110111.

**NF-e de devolução de compra** (`mod_fiscal::devolucao_compra`, 2026-10-27): a mercadoria que volta ao
fornecedor (`docs/modulos/compras.md`, `DevolverAoFornecedor`) sai numa NF-e de finalidade 4, com a nota
de compra referenciada (`refNFe`) e, na proporção do que volta, o que a compra destacou — para o fornecedor
recuperar o crédito:
- o ICMS próprio (CSOSN 900, a base na proporção e a alíquota da compra; o valor sai da base × alíquota,
  como a SEFAZ confere);
- o ST e o FCP do ST (`vBCST`, `pICMSST`, `vICMSST`, `pMVAST`, `vBCFCPST`, `pFCPST`, `vFCPST`);
- o IPI que o comércio pagou, no `impostoDevol` (`pDevol`, `vIPIDevol` — NT 2016.002);
- o frete, o seguro e as outras despesas: o que fecha o valor da devolução, repartido como na nota.

Os campos vêm do XML guardado da compra (`cardeal_fiscal::TributosDaCompra`, lidos campo a campo na
importação). CFOP 5202/6202, ou 5411/6411 quando a compra veio com ST (cobrado nela ou retido antes);
natureza "Devolução de compra para comercialização"; sem pagamento (tPag 90); indPres 0 (não se aplica);
indFinal pelo cadastro do fornecedor; nas informações complementares, a nota de origem (número, série,
data, chave), o motivo, o IPI devolvido e o texto do Simples. A NF-e soma o líquido dos itens com o
devolvido (`Nfe.devolvidos`), e fecha ao centavo com o valor da devolução no compras.

Recusada (vira rejeitada, com o porquê): no **regime normal** (o estorno do crédito de ICMS, PIS e COFINS da
compra não está no cálculo) e na nota de compra **lançada à mão**, sem chave. Quem junta os dados é o
compras (o fiscal não lê tabela de outro módulo); por isso a rejeitada se emite de novo pela devolução —
o Reenviar das Notas fiscais explica isso. Comparado com o gestao-raiz: lá a devolução é o formulário
genérico de NF-e (finalidade 4, a chave referenciada digitada), sem os tributos da compra.

**NF-e de entrada da devolução de venda** (`mod_fiscal::devolucao_venda`, 2026-10-27,
`fiscal.emitir_nfe_da_devolucao_de_venda.v1`): a mercadoria que o cliente devolveu (`docs/modulos/vendas.md`,
`ConcluirDevolucao`) entra numa NF-e emitida pela empresa (`tpNF` 0, finalidade 4) que referencia a NF-e de
venda. Os itens são os da venda guardada, na quantidade que voltou (valor e desconto na proporção); CFOP
1202/2202 — 1201/2201 da venda de produção, 1411/2411 da venda com ST (5405/6404), 1410/2410 da produção com
ST —; CSOSN 900; PIS/COFINS 98 (outras operações de entrada); sem pagamento; o destinatário e o indFinal os
da venda; a mesma série da NF-e. Só quando o cliente **não** é contribuinte do ICMS (o contribuinte emite ele
mesmo a NF-e de devolução), no Simples Nacional, e da venda que saiu em NF-e autorizada — senão a nota vira
rejeitada com o porquê. Como o fiscal tem a venda e lê a devolução pelo `mod-vendas`, o Reenviar das Notas
fiscais remonta essa nota. A criação e a remontagem são as mesmas da devolução de compra
(`devolucao_compra::sqlite::criar_ou_remontar`).

**NFS-e da ordem de serviço** (`mod_fiscal::nfse`, migração v12, 2026-10-28): a mão de obra da OS faturada
vira uma NFS-e — o DPS do Padrão Nacional, que o `sefaz-api` roteia pelo município (Nacional/ADN, Betha,
São Paulo). As peças são mercadoria e ficam de fora (saem em NF-e/NFC-e). A configuração
(`ConfigurarNfse`, `fiscal.configurar_nfse.v1`; Configurações › Fiscal › NFS-e da ordem de serviço): o código
de tributação nacional (`cTribNac`, 6 dígitos — "14.01.01" vira "140101"), o item da LC 116 ("14.01"), o
código municipal e o NBS quando o município pede, o percentual — **no Simples, o aproximado dos tributos do
Simples** (vai como `pTotTribSN`; o ISS é apurado no DAS e o DPS não leva alíquota); no regime normal, a
alíquota do ISS — e a série e o próximo número do DPS, por ambiente. A empresa precisa da inscrição
municipal nos dados fiscais.

`EmitirNfseDaOs` (`fiscal.emitir_nfse_da_os.v1`, `fiscal.emitir`) monta a nota (o tomador é o cliente da OS:
o documento, o endereço com o código do IBGE e o e-mail, cada um quando há; a discriminação é "OS n -
aparelho: serviço (valor); …") e a põe na **fila** — tabela própria, `fiscal_nfse`, porque a NFS-e tem o
número do município, a chave de 50 dígitos e eventos próprios. A fila:
- pendente → emite; autorizada guarda o número, a chave, o código de verificação e o XML;
- sem resposta (erro, processando) → **incerta**: na próxima rodada consulta pelo DPS (a série e o número
  do prestador) antes de qualquer reenvio — achou, autorizada; o município diz que não existe (404), volta
  a pendente e emite de novo, no mesmo número;
- recusada → rejeitada com o porquê; emitir de novo remonta no mesmo número;
- `CancelarNfse` (`fiscal.cancelar_nfse.v1`, `fiscal.cancelar`: o motivo do evento e a justificativa de 15
  caracteres) é um pedido que a fila leva; recusado, a nota segue autorizada com o motivo.

A vigia de fundo conta a NFS-e na fila (`ResumoFiscal.nfse_na_fila`); o pacote do contador leva os XMLs
(`Saidas/NFS-e/Autorizadas|Canceladas/<chave>-nfse.xml`, no mês da autorização) e o relatório as conta. No
gestao-raiz a NFS-e "não é suportada nesta versão". O `sefaz-api` lia a resposta da emissão no ADN num formato
que não é o da API (saía sem número, sem XML e sem a chave, que o cancelamento exige) — corrigido em
5726b25, que também faz a consulta pelo DPS buscar a nota pela chave. Não conferido contra o ADN de verdade.

**Calendário de obrigações** (`mod_fiscal::calendario`, `fiscal.obrigacoes_do_mes.v1`, 2026-10-28): o que vence
no mês, pelo regime da empresa (`nucleo_empresa.regime`). As datas são as da regra federal, com os dias úteis
descontando os fins de semana e os feriados nacionais em que os bancos não abrem (com a Sexta-feira Santa e a
segunda e a terça de Carnaval, pela Páscoa); o que **prorroga** vai para o dia útil seguinte (DAS), o que
**antecipa** para o anterior (FGTS Digital, eSocial, DCTFWeb/EFD-Reinf, PIS/COFINS).
- Simples: DAS e PGDAS-D (dia 20 do mês seguinte), DEFIS (31/03); com funcionários, DCTFWeb (dia 15).
- MEI: DAS-MEI (dia 20), DASN-SIMEI (31/05).
- Lucro presumido e real: DCTFWeb e EFD-Reinf (dia 15), PIS e COFINS (dia 25), EFD-Contribuições (10º dia
  útil do segundo mês), IRPJ e CSLL do trimestre (último dia útil do mês seguinte ao trimestre; no real, a
  estimativa mensal), ECD (último dia útil de junho), ECF (de julho); o ICMS e a EFD ICMS/IPI **sem data** — o
  prazo é do estado e da atividade.
- Com funcionários, em qualquer regime: eSocial (dia 15) e FGTS Digital (dia 20).
- Do Cardeal: inutilizar os números sem desfecho (até o dia 10, quando há) e o pacote do contador (sugestão:
  dia 5).

Na Visão geral, o cartão "Obrigações" mostra o que ainda vence no mês (e o começo do próximo, quando o mês
está acabando), com os dias que faltam; as de quem tem funcionários por último (o Cardeal não sabe se há).
Feriado estadual ou municipal não entra: o cartão pede para conferir com o contador.

O `sefaz-api` ganhou o `impostoDevol` e o FCP do ST (8075fde) — e, de quebra, uma correção: a validação
exigia `vICMSST = vBCST × pICMSST`, mas o ST é esse produto **menos** o ICMS próprio; todo ST de verdade
era recusado. Agora o ST só não pode passar do produto.

**Série por caixa** (migração v8, 2026-10-27): `DefinirSerieDoTerminal` (`fiscal.definir_serie_do_terminal.v1`,
`fiscal.configurar`) dá a um terminal a sua série (1 a 889, com nome e, se a série já foi usada em outro
sistema, o próximo número); `serie: None` o devolve à série da empresa. `SeriesDosTerminais`
(`fiscal.series_dos_terminais.v1`) lista os caixas com série e o próximo número de cada. A NFC-e da venda sai na
série do terminal do cupom — ou na da empresa, para quem não tem série própria. Regras: a série da empresa é
"a dos caixas sem série própria" e não vira a de um terminal (nem o contrário: `ConfigurarNfce` recusa a série
de um caixa); uma série é de um terminal só. Com dois caixas na mesma série a numeração não colide (o número
é reservado na transação), mas fica intercalada; e o modo autônomo (caixa sem servidor) vai exigir séries
separadas. O gestao-raiz tem uma série por empresa. Desktop: Configurações › Fiscal › "Série da NFC-e por
caixa" (este computador: nome, série, próximo número; a lista dos outros caixas, com "Tirar a série").

**Contingência off-line** (`tpEmis = 9`, migração v7, 2026-10-27). Com a SEFAZ fora e o `sefaz-api` no
ar, a nota pendente que **com certeza** não chegou à SEFAZ é assinada lá (`/nfe/nfce/contingencia/prepare`:
chave própria, `dhCont` e `xJust` no `ide`, o QR Code no `infNFeSupl`) e vai a `Contingencia`, com a nota
assinada em `xml_assinado` e o `dhCont` em `contingencia_em`. O DANFCE sai na hora com "EMITIDA EM
CONTINGÊNCIA" e "Pendente de autorização" — o consumidor leva o cupom fiscal. A fila transmite **o mesmo
XML** (`/transmit`) quando a SEFAZ voltar, sem limite de tentativas (o prazo de 24 h aparece na faixa e nas
Notas fiscais): autorizada → `Autorizada` com o `nfeProc` (a nota assinada sai); 204 (a mesma chave já
chegou) → `Incerta`, consulta com a nota assinada; 539 → confere a chave que existe; rejeitada → `Rejeitada`
(reenviar volta à emissão normal, com o mesmo número). A venda desfeita enquanto isso espera a autorização e
recebe o cancelamento.

Decisões:
- **Só a nota que com certeza não chegou** vai em contingência. O `sefaz-api` passou a distinguir
  (`naoEnviado`, 7d7faa6): circuito aberto, conexão ou TLS que não se completou, 503 e redirecionamento —
  antes, toda falha voltava com a chave, como incerta. A nota que **pode** ter chegado (timeout depois do
  envio, 500/502/504) fica `Incerta`: o mesmo número com outra chave daria duplicidade (539), e um número
  novo pede o cancelamento por substituição da original (evento 110112) — ainda não feito.
- **Automática**, sem chave na tela: a contingência é o que a lei prevê para a SEFAZ fora, e a alternativa
  era o consumidor sair sem cupom fiscal. O `xJust` é o que a SEFAZ (ou a falta dela) respondeu.
- **Sem internet nenhuma, nada muda**: a nota espera na fila. Assinar no Cardeal (XMLDSig, o XML inteiro
  em Rust) seria trazer o `sefaz-api` para dentro — não vale enquanto o caso real é a SEFAZ fora.
- O prazo de **24 horas** é o mais estrito entre as UFs (o manual fala em "até o primeiro dia útil").

**Substituição da nota sem resposta** (NT 2018.004; migração v9; `sefaz-api` 43c98f0; 2026-10-27). A nota
`Incerta` cuja consulta segue sem resultado — com a API no ar e até 15 minutos depois da venda, o cliente ainda
no balcão — ganha uma **substituta**: a mesma nota, outro número da mesma série, que sai **sempre em
contingência** (a SEFAZ exige `tpEmis` 9 da substituta); uma vez só. A substituta passa a ser a nota da venda
(a origem é única entre as vigentes: `substituida_por IS NULL`). A original segue o caminho dela:

| A consulta da original | Vai para |
|---|---|
| Autorizada | pedido `cancelar_substituicao` — o **cancelamento por substituição** (evento 110112, `chNFeRef` = a substituta), que espera a substituta estar autorizada; prazo de 168 h, que a SEFAZ confere |
| Não consta (217) | não é emitida de novo: `Rejeitada`, e o número é **inutilizado** |
| Rejeitada | o número é inutilizado |
| Sem resultado | segue conferindo, sem outra substituta |

Desfeita a venda, a substituta é a que recebe a anulação; a original resolve-se sozinha como acima. Não
conferido na SEFAZ de verdade (o leiaute do 110112 segue a NT e os exemplos do ACBr).
- Não conferido contra a SEFAZ de verdade (falta a primeira autorizada no RS); os testes usam o
  `FiscalSimulado` (`simular_sefaz_fora`) e servidores locais no `sefaz-api`.

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

**O prazo de cancelamento fica à vista (2026-10-20).** `ModeloFiscal::prazo_de_cancelamento_segundos`
(30 minutos para a NFC-e, 24 horas para a NF-e) e `DocumentoResumo::prazo_de_cancelamento(agora)` →
`PrazoDeCancelamento` (quanto passou, quanto falta, o aviso). Em Notas fiscais, o detalhe da nota
autorizada mostra "cancela por mais 18 min" ou "fora do prazo de cancelamento", e o pedido de
cancelamento traz o aviso inteiro (vermelho fora do prazo). No `F11` do PDV, a venda selecionada diz se
o cancelamento da NFC-e ainda vai dentro do prazo e, faltando 5 minutos ou menos, lembra que sem
internet o pedido espera na fila enquanto o prazo corre. **Avisa, não bloqueia** — o `gestao-raiz`
bloqueia, mas algumas UFs homologam fora do prazo (155) e quem decide é a SEFAZ; recusado (501), vale o
"cancelamento recusado" acima.
`DocumentosDasVendas` (`fiscal.documentos_das_vendas.v1`) devolve as NFC-e de várias vendas numa
consulta, para a lista do dia.

A venda **a prazo** do PDV vai na NFC-e como `tPag 05` (crédito loja).

**As notas do mês para o contador (2026-10-20).** `NotasFiscaisDoMes` (`fiscal.notas_do_mes.v1`,
`fiscal.ver`) devolve `NotasDoMes`, com três partes:
- **as notas de produção que fecharam no mês** (autorizadas, canceladas, inutilizadas, denegadas), com o
  XML da nota e o **XML do evento** (migração v5, `xml_evento`: o `procEventoNFe` do cancelamento ou o
  `procInutNFe` da inutilização, gravado no aceite quando o provedor o devolve; a guarda é obrigatória
  por 5 anos);
- **as que ainda não tiveram desfecho** (na fila, conferindo, rejeitadas), criadas até o fim do mês:
  cada uma é um buraco na série até ser autorizada ou inutilizada;
- **quantas de homologação** ficaram de fora (não têm valor fiscal).

O mês da nota é o da **emissão** (`dia_da_emissao`: o `dhEmi` do XML; sem XML, o dia da autorização),
para que a venda das 23:59 do dia 31 autorizada à 0:00 fique no mês certo; o da inutilizada é o da
inutilização. `NotaDoMes::arquivos` dá os nomes do pacote, os que os sistemas de contabilidade
conhecem: `Saidas/NFC-e/<situação>/<chave>-nfe.xml`, `110111<chave>01-procEventoNFe.xml` e
`serieNNN-numeroNNNNNNNNN-procInutNFe.xml`. `NotaDoMes::faltas` diz o que não está guardado, com o
porquê (`XmlQueFalta`):
- a nota confirmada pela consulta sem a nota assinada guardada (as de antes de 2026-10-26): a consulta não
  tem como devolver o XML — baixa-se no portal da SEFAZ;
- a nota confirmada pela consulta com a nota assinada guardada: a busca recupera;
- o cancelamento que não veio: a busca recupera, pela consulta da nota cancelada;
- a inutilização que não veio (a resposta se perdeu): não há consulta que a devolva — o protocolo comprova.

**O XML do evento e a busca do que falta (2026-10-26).** O `sefaz-api` (bc58e77) passou a devolver o
`procEventoNFe` no cancelamento e na CC-e, o `procInutNFe` na inutilização, a nota assinada no desfecho
incerto da emissão (`xmlAssinado`) e, na consulta, o `nfeProc` (se recebe a nota assinada, conferida pela
chave e pelo digest do protocolo) e o `procEventoNFe` do cancelamento (`xmlCancelamento`). O Cardeal
guarda cada um quando vem. O que não veio vai para a **busca** (migração v6, `xml_a_buscar` = tentativas
restantes): a fila consulta a nota e guarda o que a SEFAZ devolver, sem mudar o estado do documento
(`decidir_busca`, pura). Entram na busca o cancelamento aceito sem o XML ou já registrado (573/218, a
resposta do primeiro pedido se perdeu) e, pelo comando `BuscarXmlsQueFaltam`
(`fiscal.buscar_xmls_que_faltam.v1`, `fiscal.emitir`), que o pacote do contador roda antes de montar o mês,
todo documento de produção que tenha o que recuperar. A migração já pôs na busca os cancelamentos que
ficaram sem o XML. A busca encerra na primeira resposta da SEFAZ, com ou sem o XML; o erro da SEFAZ gasta
uma das 3 tentativas (a API fora do ar não gasta); e a mesma chave só volta a ser consultada depois de uma
hora (a SEFAZ barra consulta repetida — "consumo indevido", 656). A vigia do desktop também dispara a
transmissão quando há busca na hora (`ResumoFiscal::xmls_a_buscar`).

**DANFCE** (`cardeal_pdf::gerar_danfce`): bobina de 80 ou 58 mm (Configurações › Impressão; no de 58 mm a
fonte e a margem encolhem e o QR Code fica com 28 mm, acima dos 25 mm do manual) com a altura do cupom, no leiaute do manual
(emitente, itens, totais, pagamentos e troco, chave em 11 grupos com o endereço de consulta, consumidor,
número/série/emissão, protocolo e o QR Code — `qrcodegen`, MIT). Em homologação, "sem valor fiscal" no topo e
no rodapé. Montado da nota guardada + `qrCode`/`urlChave` do XML autorizado; abre no visualizador do sistema
(de onde vai à impressora). Conferido: o QR Code renderizado decodifica para a URL de consulta com a chave.

**Direto na térmica** (2026-10-27): com a impressora deste computador configurada (Configurações › Impressão),
o DANFCE vai em ESC/POS (`cardeal_pdf::danfce_escpos`), sem abrir o PDF. O layout é o **mesmo** `compor` do
PDF, escrito contra a interface `Escrita` (o PDF e o ESC/POS a implementam): as duas saídas não divergem. Só o
núcleo comum das térmicas (Epson TM, Elgin i9, Bematech em modo ESC/POS, Daruma, Tanca): `ESC @`, alinhamento,
negrito, tamanho duplo no título, corte parcial; o QR Code vai como **imagem** (`GS v 0`, 32 mm na de 80, 28 mm
na de 58), que toda ESC/POS imprime, em vez do `GS ( k`, que nem todas têm. 48 colunas na bobina de 80 mm, 32
na de 58. Acento: o padrão é sem (o texto é transliterado — sai certo em qualquer marca); ligado, a PC860
(`ESC t 3`). Não conferido numa impressora de verdade (não há uma nesta máquina): o teste manda os bytes a uma
"impressora de rede" local e confere o conteúdo.

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
**Cancelar NFC-e** e **Inutilizar número** com a justificativa; na NF-e, **Imprimir DANFE**, **Carta de
correção** e a lista das cartas com a situação) e **Transmitir agora**. No PDV, a venda
autorizada ganha **Imprimir cupom fiscal**. A nota em contingência também (o DANFCE com o aviso), tem a
etiqueta "Em contingência" e, nas Notas fiscais, entra em "Atenção" com o prazo de transmissão; a vigia de
fundo a transmite como as da fila.

## 9. Permissões

| Permissão | Risco | O que libera |
|---|---|---|
| `fiscal.ver` | Baixo | Situação fiscal, notas, teste de conexão |
| `fiscal.emitir` | Médio | Criar a NFC-e da venda, reenviar, transmitir a fila, carta de correção |
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
- v5: `fiscal_documento.xml_evento` — o XML do cancelamento (`procEventoNFe`) ou da inutilização
  (`procInutNFe`), para a guarda de 5 anos e o pacote do contador.
- v6: `fiscal_documento.xml_assinado` (a nota assinada da emissão incerta, apagada quando chega o
  `nfeProc`) e `xml_a_buscar` (tentativas restantes da busca de XML; 0 = nada a buscar); o índice da fila
  passa a cobrir a busca, e os cancelamentos sem o XML já entram nela.
- v12: `fiscal_configuracao_nfse` (o código do serviço, o percentual, a série e o próximo número do DPS por
  ambiente) e `fiscal_nfse` (a nota, a fila, o pedido de cancelamento; única por origem entre as não
  canceladas).
- v11: `fiscal_carta_correcao` (a NF-e, a sequência de 1 a 20, o texto, o estado, o motivo, o protocolo,
  o XML, as tentativas e a próxima tentativa, quando foi registrada).
- v10: `fiscal_configuracao.serie_nfe` — a série da NF-e (modelo 55), separada da da NFC-e.
- v9: `fiscal_documento` recriada com `substitui`/`substituida_por`, o pedido `cancelar_substituicao` e a
  origem única só entre as vigentes (índice parcial `fiscal_documento_origem`).
- v8: `fiscal_serie_terminal` (empresa, terminal, nome, série; uma série é de um terminal só).
- v7: `fiscal_documento` recriada com o estado `Contingencia` e `contingencia_em` (o `dhCont`); o índice
  da fila cobre a contingência.

## 11. Próximas fatias da onda 7

1. **Primeira NFC-e autorizada no RS** (falta a IE, o endereço e o CSC de homologação da SEFAZ-RS).
2. ~~A impressão direta na térmica (ESC/POS) e o DANFCE em 58 mm~~ — feitos (o 58 mm em 2026-10-15, o
   ESC/POS em 2026-10-27; `docs/modulos/pdv.md`, Comprovante).
3. ~~**Contingência offline**~~ — feita em 2026-10-27 (§7; `sefaz-api` 7d7faa6), com a **substituição** da
   nota incerta (outro número em contingência; a original cancelada por substituição, 110112, ou inutilizada;
   `sefaz-api` 43c98f0). Falta a conferência na SEFAZ de verdade.
4. ~~Série por terminal~~ — feita em 2026-10-27 (§7, "Série por caixa").
5. NF-e: ~~a de venda, o DANFE A4, a carta de correção, a devolução de compra e a NF-e de entrada da
   devolução de venda~~ (2026-10-27, §7). A devolução da venda do PDV (NFC-e) ainda não sai em NF-e.
6. ~~NFS-e da OS~~ — feita em 2026-10-28 (§7; `sefaz-api` 5726b25). Falta conferir num município de
   verdade (homologação do ADN, com a inscrição municipal e o código do serviço da empresa).
7. SPED gerado no Cardeal.
10. ~~Calendário de obrigações~~ — feito em 2026-10-28 (§7).
8. ~~DFe e manifestação~~ — feito em 2026-10-26 (sefaz-api 842576f; a busca de notas em
   `mod-compras`). Falta conferir contra o Ambiente Nacional de homologação, que pede o
   certificado com a senha; e as manifestações conclusivas (confirmação, desconhecimento).
9. ~~O `sefaz-api` devolver o XML do evento e a nota assinada da emissão incerta~~ — feito em
   2026-10-26 (sefaz-api bc58e77; aqui, a busca do que falta). Para valer em produção, o `sefaz-api`
   publicado precisa ser atualizado (o servidor dele não é desta máquina).
