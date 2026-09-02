# 08 — Segurança, usuários e permissões

## 1. Modelo de ameaças (o realista, para PME brasileira)

| Ameaça | Probabilidade | Mitigação |
|---|---|---|
| Operador dá desconto indevido / cancela venda para ficar com o dinheiro | **Alta** | Permissão granular, limite de desconto por papel, auditoria com autor, alerta de padrão anômalo |
| Funcionário demitido acessa o sistema de casa | Alta | Sessão vinculada a dispositivo registrado; revogação imediata |
| Alguém apaga movimento para esconder desvio | Alta | Nada é apagado; auditoria encadeada por hash; estorno visível |
| Senha compartilhada entre operadores | **Muito alta** | Login rápido por PIN + crachá; sessão curta no PDV; relatório de uso simultâneo |
| Notebook do vendedor roubado | Média | Posto avançado criptografado; revogação de dispositivo |
| Ransomware na rede da loja | Média | Backup externo fora do compartilhamento SMB; snapshots imutáveis |
| Sniffing na rede Wi-Fi da loja | Média | TLS obrigatório em toda comunicação, inclusive LAN |
| Concorrente compra a base do ex-funcionário | Média | Exportação em massa é permissão separada, sempre auditada e notificada ao admin |
| Ataque externo direcionado | Baixa | O servidor não é exposto à internet por padrão |

Note a ordem: **as ameaças reais são internas e cotidianas**, não hackers. O design de permissões
reflete isso.

## 2. Identidades

Três entidades distintas — confundi-las é um erro comum e caro:

| Entidade | O que é | Autenticação |
|---|---|---|
| **Usuário** | Uma pessoa | Login + senha (Argon2id), opcional TOTP |
| **Dispositivo** | Um PC/terminal | Par de chaves + certificado emitido pela CA interna |
| **Sessão** | Uma pessoa, num dispositivo, numa empresa, por um tempo | Token rotativo |

Um comando só é aceito se **os três** forem válidos. Um token roubado é inútil em outro dispositivo.

### 2.1 Senhas

- Argon2id, parâmetros `m=19 MiB, t=2, p=1` (recomendação OWASP), calibrados no primeiro boot para
  ~250 ms na máquina real.
- Nunca logamos, nunca exibimos, nunca transmitimos em claro (mesmo sob TLS, o cliente envia o
  resultado de um handshake, não a senha — ver §2.3).
- Política configurável (mínimo 8 caracteres por padrão), verificação contra lista das 10 mil senhas
  mais comuns embutida.
- Bloqueio progressivo: 5 tentativas → 1 min, 10 → 15 min, com desbloqueio pelo admin.

### 2.2 PIN de operador (o caso do PDV)

Trocar de operador no caixa 40 vezes por dia com senha de 12 caracteres não acontece na prática —
o resultado real é uma senha colada no monitor. Então:

- O operador tem, além da senha, um **PIN de 4–6 dígitos** válido **apenas** no dispositivo PDV,
  **apenas** para operações de caixa, e **apenas** enquanto o turno está aberto.
- O PIN é hasheado com o mesmo Argon2id e tem taxa limitada agressivamente (3 erros = bloqueio do
  operador naquele terminal até liberação do gerente).
- Operações de risco (cancelar cupom, desconto acima do limite, sangria) exigem **senha do supervisor**
  digitada na hora, ou aproximação de crachá NFC.

Segurança que não cabe na operação real vira insegurança. Este é o exemplo canônico.

### 2.3 Autenticação sem enviar a senha

O cliente executa um handshake do tipo desafio-resposta sobre TLS: o servidor envia um nonce, o
cliente responde com HMAC derivado da senha, e nunca transmite a senha em si. Isso protege contra
log acidental do corpo da requisição e contra um proxy mal configurado na rede do cliente.

## 3. Permissões

### 3.1 Catálogo declarativo

Permissões não são strings soltas: são **declaradas pelos módulos**, coletadas no boot e verificadas
em compilação por macro.

```rust
permissoes! {
    "financeiro.pulso.ver"          => "Ver o Pulso (tela inicial financeira)",
    "financeiro.receber.ver"        => "Consultar contas a receber",
    "financeiro.receber.criar"      => "Lançar título a receber",
    "financeiro.receber.baixar"     => "Dar baixa em recebimento",
    "financeiro.receber.estornar"   => "Estornar baixa"        [risco: Alto],
    "financeiro.pagar.autorizar"    => "Autorizar pagamento"   [risco: Alto, limite: Dinheiro],
    "financeiro.conta.editar"       => "Editar plano de contas"[risco: Alto],
    "razao.estornar"                => "Estornar lançamento"   [risco: Critico],
    "razao.reabrir_periodo"         => "Reabrir período fechado"[risco: Critico],
    "relatorio.exportar_massa"      => "Exportar dados em massa"[risco: Critico, notifica_admin],
}
```

A macro gera constantes, garante unicidade global e alimenta a tela de papéis com descrições em
português — o admin nunca vê `fin.rec.bx`.

### 3.2 Papéis de fábrica

| Papel | Para quem | Notas |
|---|---|---|
| Administrador | Dono / TI | Tudo, inclusive reabrir período |
| Gerente | Gerente de loja | Tudo operacional; sem plano de contas nem reabertura |
| Financeiro | Responsável financeiro | Financeiro completo; leitura em vendas/estoque |
| Operador de Caixa | Frente de caixa | Vender, sangria com autorização; sem ver custo nem margem |
| Vendedor | Balcão / externo | Vender, orçar, ver os próprios clientes; sem financeiro |
| Estoquista | Depósito | Entrada, inventário, transferência; sem preço de venda |
| Comprador | Compras | Cotação, pedido, entrada por XML |
| Contador | Escritório contábil | Somente leitura + exportações |
| Auditor | Auditoria | Somente leitura, inclusive auditoria e razão |

Papéis de fábrica são imutáveis (`sistema = 1`); o admin cria cópias para ajustar.

### 3.3 Escopo (a parte ABAC)

Permissão sozinha não basta. Toda verificação carrega um **escopo**:

```rust
pub struct Escopo {
    pub empresa: Id,
    pub filial: Option<Id>,
    pub caixa: Option<Id>,
    pub centro_custo: Option<Id>,
}

pub fn autorizar(sessao: &Sessao, permissao: &str, escopo: &Escopo) -> Resultado<()>;
```

Casos reais que isso resolve:
- Gerente da Filial 2 não vê o caixa da Filial 1.
- Vendedor só vê os clientes da própria carteira.
- Operador só opera o caixa em que abriu turno.
- Responsável por centro de custo aprova só o que é dele.

### 3.4 Limites por papel

Alguns "poderes" são quantitativos, não booleanos:

```rust
limites! {
    "vendas.desconto_maximo"       => Percentual,   // Operador 5%, Gerente 20%, Admin ∞
    "financeiro.pagar.teto"        => Dinheiro,     // autoriza até R$ 5.000,00
    "pdv.cancelamento_por_turno"   => Contagem,     // até 3 cancelamentos por turno
    "vendas.prazo_maximo"          => Dias,
}
```

Exceder o limite não é erro: dispara **autorização de supervisor** na hora, que é registrada
vinculando as duas identidades (quem pediu, quem autorizou).

### 3.5 Onde a verificação acontece

```
UI          → esconde o que não pode  (conveniência, não segurança)
Cliente     → não verifica nada       (não é confiável)
Servidor    → VERIFICA                (única fonte de verdade)
```

A macro de comando obriga a declaração:

```rust
#[comando(permissao = "financeiro.pagar.baixar", risco = Alto, audita)]
pub struct BaixarPagamento { /* ... */ }
```

Comando sem `permissao` **não compila**. Isso elimina a categoria de bug "esqueceram de checar".

## 4. Rede e transporte

### 4.1 CA interna

No primeiro boot o servidor gera uma CA própria (`ECDSA P-256`, validade 10 anos) e um certificado de
servidor com os IPs/hostnames locais. O cliente, ao parear pela primeira vez, **fixa** (pinning) a
impressão digital da CA.

Pareamento de um terminal novo:

```
1. Terminal descobre servidores na LAN via mDNS (_cardeal._tcp).
2. Terminal exibe o fingerprint do servidor: "A3F2 91C4 ... 7B10".
3. Admin confere o mesmo fingerprint na tela do servidor e digita um código de 6 dígitos
   válido por 5 minutos.
4. Servidor emite certificado para o terminal e registra o dispositivo.
5. A partir daí, mTLS: os dois lados se autenticam por certificado.
```

Zero configuração de rede pelo usuário, e ainda assim resistente a man-in-the-middle na LAN.

### 4.2 Sessões

- Token de acesso de vida curta (15 min) + token de renovação rotativo.
- Reuso de token de renovação = sinal de roubo → invalida toda a família de tokens e alerta.
- Sessão vinculada ao certificado do dispositivo; token em outro dispositivo é rejeitado.
- No PDV, a sessão do **terminal** dura o turno; a do **operador** é uma camada leve por cima.

## 5. Auditoria

Toda operação marcada com `audita` grava em `nucleo_auditoria`: autor, dispositivo, horário, ação,
entidade, e o **diff de campos alterados** (não a linha inteira — economiza espaço e é mais legível).

A cadeia de hash (ver [doc 06 §2.1](06-modelo-de-dados.md)) torna a tabela à prova de adulteração.

### 5.1 Alertas comportamentais

Além do log passivo, o motor observa padrões e avisa o gestor:

| Padrão | Alerta |
|---|---|
| Cancelamentos do operador X acima de 3 desvios-padrão da equipe | "Verificar cancelamentos no Caixa 2" |
| Desconto médio de um vendedor subindo consistentemente | "Desconto médio de Y subiu 40% em 30 dias" |
| Sangrias fora do padrão de horário | "Sangria às 23h14 — fora do horário de operação" |
| Exportação em massa de clientes | Notificação imediata ao administrador |
| Acesso de dispositivo novo | Notificação imediata |
| Estorno de lançamento em período antigo | Notificação imediata |

São heurísticas simples e explicáveis, mostradas com os dados que as motivaram — nunca acusação
automática. O objetivo é dar visibilidade ao dono, não policiar o funcionário.

## 6. LGPD

| Requisito | Implementação |
|---|---|
| Base legal e finalidade | Documentada por campo em `docs/lgpd/inventario-dados.md` |
| Minimização | Não coletamos o que não usamos. Cadastro de cliente no PDV é opcional |
| Direito de acesso | `Exportar dados do titular` gera JSON completo do que temos daquele CPF |
| Direito de eliminação | Anonimização (nome → "Titular removido", CPF → hash) preservando o razão, que é obrigação fiscal |
| Registro de tratamento | A auditoria já é o registro |
| Segurança | Criptografia em repouso nos terminais, TLS em trânsito, controle de acesso granular |
| Incidente | Runbook em `docs/runbooks/incidente-dados.md` |

**Nota importante:** eliminação de dado pessoal **não** apaga lançamentos contábeis — a legislação
fiscal (guarda de 5 anos) prevalece. Anonimizamos o cadastro e mantemos o fato financeiro.
Isso é explicado ao titular na resposta ao pedido.

## 7. Práticas de código

- `#![forbid(unsafe_code)]` em todos os crates exceto onde houver FFI justificada (drivers) — e lá,
  isolada em módulo `unsafe` com comentário `SAFETY:` obrigatório.
- `cargo deny` e `cargo audit` no CI, bloqueando vulnerabilidade conhecida.
- Segredos nunca em `Debug`: tipos `Segredo<T>` com `Debug` que imprime `[oculto]`.
- Zeroização de material criptográfico (`zeroize`) ao sair de escopo.
- Comparação de segredos em tempo constante (`subtle`).
- Nenhum dado pessoal em log. O logger filtra CPF/CNPJ/e-mail/telefone por padrão e o teste
  `logs_sem_pii` valida isso.
