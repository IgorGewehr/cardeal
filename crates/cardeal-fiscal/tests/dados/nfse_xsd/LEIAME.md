# Esquemas oficiais da NFS-e (padrão nacional)

Cópias sem alteração dos XSD publicados em <https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica>:

- `vigente/`: `nfse-esquemas_xsd-v1-01-20260209.zip` (documentação atual, produção);
- `nt009/`: `esquemas_prod_restrita_20261020.zip` (produção restrita, NT 009).

Os testes de `cardeal-fiscal` (`nfse_nacional`) validam a DPS e o pedido de cancelamento contra os
dois conjuntos com o `xmllint`, e conferem a assinatura com o `xmlsec1`.

Um defeito do esquema vigente: o `TSSerieDPS` traz o padrão `^0{0,4}\d{1,5}$`. Em XSD, `^` e `$`
não são âncoras: o validador da Sefin (.NET) os trata como âncoras, mas o libxml2 os lê como
caracteres e recusaria toda série. O teste troca só esse padrão pelo equivalente sem `^`/`$` na
cópia temporária que valida; os arquivos daqui ficam como publicados.
