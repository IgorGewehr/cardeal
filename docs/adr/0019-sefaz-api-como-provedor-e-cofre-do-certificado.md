# ADR-0019 — O `sefaz-api` é o provedor fiscal; o certificado A1 mora cifrado no cofre da instalação

- **Status:** Aceita
- **Data:** 2026-10-14
- **Decisores:** Usuário (produto) + equipe
- **Relacionadas:** ADR-0006, ADR-0016

## Contexto

O ADR-0006 decidiu que o Cardeal não emite direto na SEFAZ: fala com uma API fiscal atrás de uma
porta (`PortaFiscal`), e a escolha do provedor ficou em aberto — o roadmap travava a onda 7 nela
(NFe.io, Tecnospeed, Focus...). A operação já tem um motor fiscal próprio, o **`sefaz-api`**
(Node/Express, publicado em `https://emissao.tensorroot.com`), em produção para o `saas-erp`, o
`next-erp` e o Supermarket-OS. Ele é **sem estado**: recebe o certificado A1 (`.pfx` em base64 e a
senha) no corpo de toda chamada e não guarda nada. Logo, alguém tem de guardar o certificado — e o
Cardeal roda tanto num desktop monoposto quanto num servidor multi-tenant (ADR-0016), com a base indo
para backup e réplica.

## Decisão

1. **O provedor é o `sefaz-api`, branch `main`.** O adaptador `ApiFiscalHttp` (`cardeal-fiscal`) fala o
   contrato dele; a porta continua sendo a fronteira, e o "provedor" virou um recurso injetado
   (`ProvedorFiscal`), não uma linha de configuração com nomes de empresas. O que o `sefaz-api` não tem
   (DFe, manifestação) cresce **nele**; o que é dado do Cardeal (DANFE, SPED, tributos) fica no Cardeal.
2. **O certificado mora no Cardeal, cifrado** com AES-256-GCM por um `Cofre` cuja chave **não está na
   base**: variável de ambiente no servidor (`CARDEAL_CHAVE_SEGREDOS`), arquivo `cofre.chave` na pasta de
   dados no desktop. O selado é amarrado à empresa (dado associado). A base só tem texto cifrado.
3. **Conferir antes de guardar**, em Rust puro (sem OpenSSL): senha, validade, CNPJ igual ao da empresa
   (o `sefaz-api` compara exato e recusaria na emissão) — e **regravar os PFX com RC2**, que o TLS do
   `sefaz-api` (OpenSSL 3) recusa. O A1 real da Tensorroot é RC2; sem isso, nenhuma nota sairia.
4. **Recursos no despacho:** `Ctx::recurso::<T>()` sobre `cardeal_modkit::Recursos` (mapa por tipo,
   montado por quem cria o motor) — o "`Ctx::porta()` adiado" do contrato, generalizado.

## Alternativas consideradas

- **Provedor comercial (NFe.io, Tecnospeed, Focus):** custo por documento e mais um fornecedor, sem
  ganho de cobertura sobre o que o `sefaz-api` já faz em produção. O motor próprio já carrega as
  correções de campo (contingência NFC-e, SVC, NFref, auditoria P0–P2).
- **`sefaz-api` v2 (NestJS, guarda o certificado):** parada desde maio e sem as correções da `main`.
  Quando (e se) for publicada, o Cardeal ganha um segundo adaptador; a porta já isola isso.
- **Guardar a chave do cofre na própria base:** inútil — quem copia o `.db` levaria as duas coisas.
- **SQLCipher na base inteira:** continua previsto (doc 07 §8) para terminais expostos, mas não resolve o
  servidor (chave no mesmo disco) nem dispensa a guarda separada do segredo mais sensível.
- **Converter o PFX com OpenSSL/forge:** exigiria OpenSSL nativo ou Node no cliente; o leitor/escritor
  PKCS#12 em Rust tem ~1 mil linhas testadas contra arquivos de quatro exportadores.

## Consequências

- Positivas: a onda 7 destravou sem contrato novo; o certificado nunca está em claro em disco nem em
  backup; o mesmo código serve desktop e servidor; trocar de provedor é trocar o recurso.
- Negativas: a chave do cofre no desktop fica em arquivo ao lado da base (quem leva o disco inteiro leva
  a chave) — mitigação planejada: DPAPI/Keychain; a chave da API fiscal precisa chegar a cada instalação
  (`CARDEAL_FISCAL_TOKEN`) — no modo servidor fica só no servidor; restaurar a base em outra máquina sem
  a chave exige reinstalar o certificado (a mensagem diz isso).
- O PKCS#12 próprio é código de segurança nosso: os testes cobrem AES-256/128, 3DES, RC2-40, MAC
  SHA-1/256, AC junto, senha errada, arquivo truncado e o vetor conhecido da derivação de chave.

## Quando revisitar

- Se o `sefaz-api` v2 (com guarda de certificado) for publicado e se mostrar estável.
- Se a operação passar a vender o Cardeal para quem não pode depender do `sefaz-api` da Tensorroot.
- Quando o desktop ganhar armazenamento de chave do sistema (DPAPI/Keychain) — o arquivo `cofre.chave`
  passa a ser só o fallback.
