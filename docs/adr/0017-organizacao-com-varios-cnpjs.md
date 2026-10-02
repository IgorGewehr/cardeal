# ADR-0017 — Organização com vários CNPJs numa base só

- **Status:** Aceita
- **Data:** 2026-10-02
- **Decisores:** Igor Gewehr
- **Relacionadas:** ADR-0004, ADR-0012, ADR-0016

## Contexto

Clientes reais têm mais de um CNPJ: matriz e filiais, ou um grupo de empresas do mesmo dono. A
ADR-0016 abriu **uma base por empresa** no servidor. Com isso, matriz e filial ficam em
arquivos separados:
- nenhuma operação entre elas é atômica (transferência de estoque, por exemplo);
- relatório consolidado exige varrer N arquivos;
- o mesmo dono precisa de um vínculo por CNPJ.

O esquema, porém, já nasceu pronto para isso:
- toda tabela de domínio tem a coluna `empresa`, e toda consulta filtra por ela (auditado em
  2026-10-02);
- toda unicidade é por empresa ou por registro pai;
- `nucleo_empresa` tem `matriz`;
- o usuário é global na base, e papéis e sessões são por empresa;
- o despachante só autoriza quando a empresa da sessão (`Escopo`) é a do ambiente.

O que prendia em uma empresa era só o motor (`SELECT id FROM nucleo_empresa LIMIT 1`) e
quatro consultas do `mod-empresa`.

## Decisão

**Uma base = uma organização.** Dentro dela ficam N empresas (CNPJs): a primeira é a matriz,
e o id dela é o id da base. Organizações diferentes continuam em bases diferentes, com
isolamento físico (ADR-0016 inalterada).

- **Motor:**
  - a sessão é por empresa (`sessao_do_usuario_em`, `autenticar_em`);
  - o ambiente de cada chamada sai da sessão, e não de uma empresa fixa;
  - `empresas()` lista os CNPJs da base;
  - `adicionar_empresa` cria um CNPJ com plano de contas próprio e papel Administrador
    próprio, atribuído a quem criou;
  - a sincronização do admin roda para todas as empresas.
- **Dados:** cada CNPJ tem os seus. Financeiro, fiscal, estoque, OS e numerações são por CNPJ,
  como a lei e a contabilidade exigem.
- **Usuários:** um usuário da organização entra nas empresas em que tem papel; "usuários da
  empresa" = quem tem papel nela.
- **Servidor:**
  - o diretório guarda a base de cada empresa (`diretorio_empresa.base`; nas bases
    existentes, base = id da empresa);
  - a frota abre bases, e as rotas continuam `/v1/e/{empresa}/…`;
  - o vínculo continua por empresa (conta → empresa → usuário), então dar acesso a uma filial
    = mais um vínculo;
  - `POST /v1/e/{empresa}/empresas` cria um CNPJ na mesma organização e vincula quem pediu.
- **Tempo real:** a leitura do outbox filtra pela empresa da conexão.

## Fora desta decisão (próximo passo, decisão de produto)

**Cadastros compartilhados entre os CNPJs** (clientes/fornecedores e catálogo de produtos
comuns ao grupo, com saldo de estoque por CNPJ). É o comum nos ERPs, e esta base única é
justamente o que o torna possível. Exige definir, módulo a módulo, o que é do grupo e o que é
do CNPJ. Hoje cada CNPJ tem os próprios cadastros.

## Consequências

- Operação atômica entre CNPJs e relatório consolidado passam a ser uma transação ou uma
  consulta, quando forem feitos.
- Uma organização grande ocupa um arquivo maior e um escritor só: o group commit
  (ADR-0012) segura milhares de commits por segundo, muito acima de qualquer grupo PME.
- Bases existentes não mudam: uma organização com um CNPJ é exatamente o que já existia.

## Quando revisitar

- Uma organização ultrapassar a vazão de um escritor (medir antes).
- Exigência de isolamento físico entre CNPJs do mesmo grupo (por exemplo, sócios diferentes).
