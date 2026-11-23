# ADR-0021 — A NFC-e em contingência é montada e assinada pelo próprio Cardeal

- **Status:** Aceita
- **Data:** 2026-11-23
- **Decisores:** Usuário (produto: "emissão em contingência impecável") + equipe
- **Relacionadas:** ADR-0006, ADR-0019, ADR-0020

## Contexto

A contingência off-line da NFC-e (`tpEmis = 9`, 2026-10-27) dependia do `sefaz-api`: era ele que montava e
assinava a nota (`/nfe/nfce/contingencia/prepare`). Cobria a SEFAZ fora, mas não o caso mais comum num
caixa — **a internet do caixa cair**. Aí não havia quem assinasse, a nota ficava na fila e o consumidor saía
sem o cupom fiscal. A decisão de então ("assinar no Cardeal seria trazer o `sefaz-api` para dentro — não
vale") pesava o XMLDSig e o XML inteiro em Rust.

O ADR-0020 trouxe o XMLDSig para o Cardeal (a DPS da NFS-e: C14N, RSA-SHA1, o A1 do cofre), conferido
por um verificador independente. O custo que pesava sumiu: falta só o XML da NFC-e, que é pequeno no que o
Cardeal emite (Simples e regime normal sem ST).

## Decisão

1. **A NFC-e em contingência sai do Cardeal** (`cardeal_fiscal::montar_nfce_em_contingencia`): a chave com
   `tpEmis = 9`, o `infNFe` na ordem do XSD 4.00, o `infNFeSupl` com o QR Code v3 off-line (os parâmetros
   `chNFe|3|tpAmb|dia|vNF|tpIdDest|cDest` assinados com a chave do A1) e a `Signature`. É **o mesmo XML**
   que o `sefaz-api` monta para o mesmo pedido — conferido em teste contra o montador dele (só o
   `verProc` difere) e contra o XSD oficial (`tests/dados/nfe_xsd`).
2. **Local primeiro, sempre**: a porta da empresa (`PortaComNfseNacional`) monta a contingência aqui, sem
   rede; só sem o responsável técnico ou com o que a montagem local não cobre (CSOSN/CST fora de 102, 103,
   300, 400, 500 / 00, 20, 40, 41, 50, 60) pede ao `sefaz-api`. A transmissão continua no `sefaz-api`
   (`/contingencia/transmit`, o mesmo XML).
3. **"Sem conexão" é um erro próprio** (`ErroFiscal::ApiFiscalInalcancavel`, `CodigoErro::FISCAL_INALCANCAVEL`):
   a conexão com a API nem se completou (DNS, recusa, TLS, tempo para conectar; o 530 do Cloudflare com o
   túnel fora). O pedido **com certeza** não saiu. A API que conectou e não respondeu segue
   `ServicoIndisponivel` — o pedido pode ter saído.
4. **Só a nota que com certeza não saiu**: sem conexão, a NFC-e pendente vai em contingência se nunca foi
   tentada ou se toda tentativa anterior também terminou sem conexão (o código `SEMREDE` gravado) ou com
   resposta. A que um dia conectou e ficou sem resposta espera a rede — outra com o mesmo número em
   contingência seria duplicidade.
5. **Uma rodada sem conexão não volta à rede**: as outras NFC-e pendentes da rodada vão em contingência
   na hora, e o resto (consultas, transmissões, pedidos) espera.
6. **Responsável técnico** (`infRespTec`, obrigatório): o da última nota autorizada da empresa (foi o
   `sefaz-api` que a montou), ou as variáveis `CARDEAL_RESP_TEC_CNPJ/CONTATO/EMAIL/FONE`.

## Consequências

- O caixa sem internet emite: o DANFCE sai com "EMITIDA EM CONTINGÊNCIA", e a nota vai quando a rede voltar
  (prazo de 24 h).
- O certificado vencido também impede a contingência local (a porta não abre o A1 vencido).
- Mudança no leiaute da NFC-e (uma NT nova) passa a pedir ajuste aqui **e** no `sefaz-api`; o teste de
  conferência entre os dois (`exportar_para_conferir_com_o_sefaz_api`, manual) mostra a diferença.
- A primeira nota da empresa sem nenhuma autorizada antes, e sem as variáveis, ainda depende do
  `sefaz-api` para a contingência.
