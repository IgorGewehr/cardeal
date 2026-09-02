# 09 — Protocolo e API

## 1. Princípios

1. **Contrato tipado compartilhado.** Servidor e cliente compilam o mesmo crate `cardeal-protocol`.
   Uma incompatibilidade é erro de compilação, não erro em produção.
2. **Três verbos, não trinta rotas.** `/cmd`, `/qry`, `/ev`. A semântica está no envelope, não na URL.
3. **Idempotência obrigatória em comandos.** Toda escrita carrega chave.
4. **Push, nunca polling.** Mudanças chegam ao cliente por WebSocket.
5. **Degradação graciosa.** O cliente funciona sem o servidor (modo autônomo).

## 2. Envelopes

```rust
pub struct EnvelopeComando {
    pub protocolo: u16,              // versão do protocolo
    pub chave: ChaveIdempotencia,    // UUIDv7
    pub empresa: Id,
    pub dispositivo: Id,
    pub emitido_em: Instante,
    pub comando: Comando,            // enum não exaustivo
}

#[non_exhaustive]
pub enum Comando {
    // financeiro
    LancarTitulo(financeiro::LancarTitulo),
    BaixarParcela(financeiro::BaixarParcela),
    AbrirCaixa(financeiro::AbrirCaixa),
    // pdv
    FinalizarVenda(pdv::FinalizarVenda),
    CancelarCupom(pdv::CancelarCupom),
    // ...
}

pub enum RespostaComando {
    Ok   { versao: Versao, carga: CargaResposta, eventos: Vec<ResumoEvento> },
    Erro { codigo: CodigoErro, mensagem: String, campo: Option<String>, detalhes: Detalhes },
}
```

## 3. Erros: como o usuário os vê

Erro não é `500 Internal Server Error`. Todo erro carrega **o que aconteceu, por que, e o que fazer**.

```rust
pub struct Detalhes {
    /// Frase curta e humana, em português, sem jargão. Vai no título do diálogo.
    pub resumo: String,
    /// O que causou. Aparece no corpo.
    pub causa: String,
    /// Ações possíveis. Viram botões.
    pub acoes: Vec<AcaoSugerida>,
    /// Identificador para o suporte. Aparece pequeno, copiável.
    pub correlacao: Id,
}
```

Exemplo real:

```
┌─────────────────────────────────────────────────────────┐
│  Não foi possível baixar esta parcela                   │
│                                                          │
│  O caixa "Caixa 1" foi fechado às 18:03 por João Silva. │
│  Baixas precisam de um caixa aberto.                     │
│                                                          │
│  [Abrir caixa]  [Escolher outro caixa]  [Cancelar]      │
│                                        ref: 019...a4f2   │
└─────────────────────────────────────────────────────────┘
```

Compare com `ERRO: FK constraint failed`. A diferença entre um ERP que gera chamado de suporte e um
que não gera está quase toda aqui.

### Catálogo de códigos

| Faixa | Categoria | Exemplo |
|---|---|---|
| `1xxx` | Validação de entrada | `1003 CampoObrigatorio` |
| `2xxx` | Regra de negócio | `2014 CaixaFechado`, `2031 EstoqueInsuficiente` |
| `3xxx` | Autorização | `3001 SemPermissao`, `3004 LimiteExcedido` |
| `4xxx` | Concorrência | `4001 VersaoDesatualizada`, `4002 RecursoTravado` |
| `5xxx` | Infraestrutura | `5002 BancoIndisponivel` |
| `6xxx` | Integração externa | `6101 SefazIndisponivel`, `6203 TefRecusado` |

## 4. Transporte

| Modo | Transporte | Serialização | Latência típica |
|---|---|---|---|
| Monoposto | Chamada direta em memória | nenhuma | ~5 µs |
| LAN | HTTP/2 sobre mTLS, conexão persistente | postcard | ~0,8 ms |
| Externo / integração | HTTP/1.1 | JSON | — |
| Tempo real | WebSocket sobre a mesma conexão | postcard | push |

A negociação é por `Content-Type`. O modo binário (`application/x-cardeal-postcard`) é o padrão
entre nossos componentes; JSON existe para integração de terceiros e para depuração
(`?formato=json` em desenvolvimento).

**postcard** foi escolhido por ser compacto (sem nomes de campo), zero-copy na deserialização e
implementado em `no_std` — o mesmo formato serve para o log durável em disco, o outbox e a rede.
Ver [ADR-0008](adr/0008-postcard-como-serializacao-interna.md).

## 5. Consultas

```rust
pub struct EnvelopeConsulta {
    pub protocolo: u16,
    pub empresa: Id,
    pub consulta: Consulta,
    pub pagina: Option<Cursor>,
    pub limite: u16,               // teto de 500, padrão 60
    pub apos_versao: Option<Versao>, // "read your writes"
}

pub struct Pagina<T> {
    pub itens: Vec<T>,
    pub proximo: Option<Cursor>,
    pub total_aproximado: Option<u64>,  // só quando barato de calcular
    pub versao: Versao,
}
```

**Cursor keyset**, nunca `OFFSET`. O cursor é opaco (base64 de postcard) e contém as chaves de
ordenação da última linha. Isso mantém a paginação estável mesmo com inserções concorrentes e o custo
constante na página 1 e na página 10.000.

## 6. Tempo real

```rust
pub enum EventoServidor {
    Dominio(EventoDominio),                 // "vendas.pedido_faturado.v1"
    Projecao { tela: Tela, versao: Versao },// "o Pulso mudou, recarregue"
    Sistema(EventoSistema),                 // energia, backup, atualização
    Presenca { usuario: Id, onde: Tela },   // quem está vendo o quê
}
```

O cliente **assina por assunto**, não recebe tudo:

```rust
cliente.assinar(&["financeiro.pulso", "pdv.caixa:7", "estoque.saldo:*"])?;
```

Isso evita que 20 terminais recebam eventos irrelevantes — economia direta de CPU e rede em cada
terminal, o que importa para o Pilar I.

### Reconexão

Cada evento tem `seq`. Na reconexão o cliente informa o último `seq` visto e recebe o intervalo
faltante (retido por 5 minutos no servidor). Se o intervalo expirou, o servidor responde
`RessincronizacaoNecessaria` e o cliente recarrega o estado.

## 7. Versionamento

- `protocolo: u16` no envelope. O servidor aceita `[atual-2, atual]`.
- Comandos e eventos versionados no nome (`vendas.pedido_faturado.v1`).
- Campo novo opcional = compatível. Campo obrigatório novo ou semântica alterada = nova versão.
- `#[non_exhaustive]` em todos os enums públicos do protocolo: cliente antigo lidando com variante
  desconhecida faz fallback controlado em vez de entrar em pânico.
- Teste de compatibilidade: a suíte guarda envelopes serializados de versões anteriores em
  `testes/ouro/protocolo/` e valida que ainda deserializam.

## 8. API externa (integração)

Superfície pequena e estável, para e-commerce, aplicativo do cliente, marketplace e BI de terceiros:

| Recurso | Métodos |
|---|---|
| `/api/v1/catalogo` | listar produtos, preços, saldo |
| `/api/v1/pedidos` | criar, consultar, cancelar |
| `/api/v1/clientes` | criar, consultar, atualizar |
| `/api/v1/titulos` | consultar contas a receber, baixar |
| `/api/v1/webhooks` | assinar eventos de domínio |
| `/api/v1/relatorios` | fluxo de caixa, DRE, curva ABC (JSON e Parquet) |

Autenticação por chave de API com escopo e limite de taxa. Toda chave é vinculada a um papel — a API
externa passa **pelo mesmo verificador de permissões** do sistema. Não há caminho privilegiado.

## 9. Descoberta na rede

O servidor anuncia `_cardeal._tcp.local` com TXT `versao`, `empresa`, `fingerprint`. O terminal novo
lista os servidores encontrados e o admin escolhe. Sem digitar IP, sem configurar porta, sem
`hosts` — em uma loja o instalador não é um administrador de redes.
