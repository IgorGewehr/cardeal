# ADR-0020 — A NFS-e do padrão nacional é emitida pelo próprio Cardeal, direto na Sefin Nacional

- **Status:** Aceita
- **Data:** 2026-11-23
- **Decisores:** Usuário (produto) + equipe
- **Relacionadas:** ADR-0006, ADR-0019

## Contexto

O ADR-0006 delegou a emissão a uma API externa porque a NFS-e tinha "cerca de 5.570 padrões — um por
município". Isso deixou de valer: desde a LC 214/2025, a NFS-e de padrão nacional é **uma API só**
(Sefin Nacional / ADN), com um leiaute versionado e publicado (DPS 1.01, notas técnicas 004 a 009),
e quase todos os municípios aderiram a ela. Quem fica de fora são poucos sistemas próprios (São Paulo) e
os municípios que emitem pelo Betha Cloud.

O `sefaz-api` (ADR-0019) montava a DPS ainda no leiaute **1.00** e, ao conferir contra os XSD
oficiais, tinha erros que fariam o município recusar ou tributar errado: `opSimpNac=1` (que é "não
optante") para empresa do Simples, `regEspTrib=6` ("sociedade de profissionais") por padrão, o
desconto e as informações complementares fora dos grupos do esquema e a ordem dos campos do tomador
trocada. A NT 009 (produção restrita desde 20/10/2026) muda de novo o leiaute (o `finNFSe` dentro do
`infDPS`, o NBS antes da descrição).

## Decisão

1. **A NFS-e de município do padrão nacional sai do Cardeal**, sem intermediário
   (`cardeal_fiscal::nfse_nacional`): a DPS é montada no leiaute 1.01 (vigente e NT 009), assinada com o
   A1 da empresa e enviada à Sefin Nacional por TLS mútuo. São Paulo e os municípios no Betha seguem pelo
   `sefaz-api`, que continua com a NF-e, a NFC-e e o DF-e. A escolha é por município
   (`emite_pela_sefin_nacional`), dentro da porta (`PortaComNfseNacional`) — o resto do sistema não sabe.
2. **Assinatura como o manual manda** (Manual de integração, §6.1.4): XMLDSig *enveloped*, C14N 1.0,
   `rsa-sha1`/`sha1`, só o certificado do titular no `KeyInfo`. O XML é escrito já canônico (sem
   canonicalizador genérico); a chave RSA sai do PKCS#12 pelo leitor próprio (ADR-0019) e assina pelo
   crate `rsa` (RustCrypto, com cegamento).
3. **TLS nativo só para a Sefin.** O IIS da Sefin pede o certificado do cliente numa *renegociação* TLS
   1.2, depois de receber a requisição; o rustls não renegocia (a conexão cai com "reset"). O cliente da
   Sefin usa o TLS do sistema (SChannel no Windows, OpenSSL no Linux); o resto do Cardeal segue no rustls.
   O servidor passa a precisar da `libssl` (imagem `distroless/base`).
4. **Conferir o certificado no padrão ICP-Brasil ao instalar**: X.509 v3, não ser de AC, `keyUsage` com
   assinatura digital e não repúdio, "autenticação de cliente" quando há `extendedKeyUsage`, o
   CNPJ/CPF ICP-Brasil, a validade, e a chave privada abrir e ser a do certificado. A raiz fora da
   ICP-Brasil é aviso (a homologação aceita certificados de teste).
5. **Duas versões do leiaute 1.01** convivem: a vigente na produção e a NT 009 na produção restrita
   (`CARDEAL_NFSE_LEIAUTE` força uma). Se a Sefin recusar pelo esquema (E1235), a DPS é remontada no outro
   leiaute e reenviada uma vez, e o processo passa a usar o que passou — a virada da produção para a NT 009
   não para a emissão.
6. **Uma nota por DPS**: toda recusa confere `GET /dps/{id}` antes de virar rejeição (o reenvio de uma
   DPS que já virou nota recupera a nota em vez de duplicar); a fila consulta pelo DPS quando a resposta
   se perde (como já fazia).

## Alternativas consideradas

- **Só corrigir o `sefaz-api`:** resolve os erros do leiaute, mas a NFS-e continuaria dependendo de um
  serviço de outra máquina, com o certificado viajando a cada chamada, e a virada da NT 009 dependeria de
  publicar o `sefaz-api`. Ele é corrigido também (branch própria), para os outros ERPs que o usam.
- **rustls para tudo:** não fala com a Sefin (sem renegociação).
- **OpenSSL vendorizado:** dispensaria a `libssl` no servidor, mas compila o OpenSSL a cada build e pede
  ferramentas no Dockerfile; a `libssl` do sistema é atualizada pela distribuição.

## Consequências

- Positivas: a NFS-e nacional funciona sem a chave do `sefaz-api`; o certificado não sai da instalação;
  a DPS é validada nos testes contra os **XSD oficiais** (vigente e NT 009, `xmllint`) e a assinatura por
  um verificador independente (`xmlsec1`); a conexão foi conferida contra a produção restrita da Sefin (a
  renegociação acontece e a recusa de um certificado de teste chega como 403 legível).
- Negativas: o leiaute nacional passa a ser nosso — cada nota técnica nova é trabalho daqui; Linux e o
  servidor dependem da `libssl`; o grupo IBS/CBS (opcional nos dois leiautes) ainda não é enviado.
- O DANFSe: a API de DANFSe do ADN (v1) foi desativada em 03/08/2026; a geração local segundo a NT 008
  fica para depois.

## Quando revisitar

- Quando a produção passar à NT 009 e o leiaute vigente sair: remover o leiaute antigo.
- Quando o grupo IBS/CBS passar a ser exigido para os serviços da base (cronograma da reforma).
- Se o rustls ganhar renegociação, ou a Sefin passar a pedir o certificado no aperto de mão inicial (como
  o ADN já faz): voltar ao rustls e tirar a `libssl`.
