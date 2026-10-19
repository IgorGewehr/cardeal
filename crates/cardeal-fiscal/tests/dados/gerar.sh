#!/usr/bin/env bash
# Gera os certificados A1 de TESTE usados por `certificado.rs` — chaves descartáveis,
# emitidas por uma AC fictícia; nada aqui é um certificado ICP-Brasil de verdade.
# Uso: bash gerar.sh (precisa de OpenSSL >= 3.4, por -not_before/-not_after).
set -euo pipefail
cd "$(dirname "$0")"
export MSYS_NO_PATHCONV=1
t=$(mktemp -d ./tmp.XXXXXX)  # relativo: com MSYS_NO_PATHCONV, um /tmp não chega ao openssl do Windows
trap 'rm -rf "$t"' EXIT
senha=cardeal-teste

openssl req -x509 -newkey rsa:2048 -nodes -keyout "$t/ac.key" -out "$t/ac.pem" \
  -subj "/C=BR/O=AC Ficticia de Teste/CN=AC TESTE CARDEAL" \
  -not_before 20250101000000Z -not_after 20450101000000Z 2>/dev/null

folha() { # nome subj ext not_before not_after
  openssl req -new -newkey rsa:2048 -nodes -keyout "$t/$1.key" -out "$t/$1.csr" -subj "$2" 2>/dev/null
  printf '%s\n' "$3" > "$t/$1.ext"
  openssl x509 -req -in "$t/$1.csr" -CA "$t/ac.pem" -CAkey "$t/ac.key" -set_serial "0x$(openssl rand -hex 8)" \
    -extfile "$t/$1.ext" -not_before "$4" -not_after "$5" -out "$t/$1.pem" 2>/dev/null
}

ext_cnpj='basicConstraints=CA:FALSE
keyUsage=digitalSignature,nonRepudiation,keyEncipherment
subjectAltName=otherName:2.16.76.1.3.4;OCTETSTRING:3010198012345678901000000000000000SSPSP,otherName:2.16.76.1.3.2;PRINTABLESTRING:FULANO RESPONSAVEL,otherName:2.16.76.1.3.3;OCTETSTRING:11222333000181,otherName:2.16.76.1.3.7;OCTETSTRING:000000000000,email:fiscal@empresa.test'
folha empresa "/C=BR/O=ICP-Brasil/OU=Certificado PJ A1/CN=EMPRESA TESTE LTDA:11222333000181" \
  "$ext_cnpj" 20250301120000Z 20350301120000Z
folha vencido "/C=BR/O=ICP-Brasil/CN=EMPRESA VENCIDA LTDA:11444777000161" \
  'basicConstraints=CA:FALSE' 20200101000000Z 20210101000000Z
folha pessoa "/C=BR/O=ICP-Brasil/CN=FULANO DE TAL:12345678909" \
  'basicConstraints=CA:FALSE' 20250101000000Z 20350101000000Z

# OpenSSL 3 padrão: PBES2 (PBKDF2/HMAC-SHA256, AES-256-CBC), MAC SHA-256 — a AC na frente.
openssl pkcs12 -export -inkey "$t/empresa.key" -in "$t/empresa.pem" -certfile "$t/ac.pem" \
  -name "empresa" -out aes256.pfx -passout "pass:$senha"
# Legado (OpenSSL 1.x / exportação antiga): certificado em RC2-40, chave em 3DES, MAC SHA-1.
openssl pkcs12 -export -legacy -inkey "$t/empresa.key" -in "$t/empresa.pem" \
  -out legado.pfx -passout "pass:$senha"
# Exportação do Windows ("TripleDES-SHA1"): tudo em 3DES.
openssl pkcs12 -export -inkey "$t/empresa.key" -in "$t/empresa.pem" -certpbe PBE-SHA1-3DES \
  -keypbe PBE-SHA1-3DES -macalg sha1 -out 3des.pfx -passout "pass:$senha"
# PBES2 com AES-128 e MAC SHA-1.
openssl pkcs12 -export -inkey "$t/empresa.key" -in "$t/empresa.pem" -certpbe AES-128-CBC \
  -keypbe AES-128-CBC -macalg sha1 -out aes128.pfx -passout "pass:$senha"
openssl pkcs12 -export -inkey "$t/vencido.key" -in "$t/vencido.pem" -out vencido.pfx -passout "pass:$senha"
openssl pkcs12 -export -inkey "$t/pessoa.key" -in "$t/pessoa.pem" -out pessoa.pfx -passout "pass:$senha"
echo ok
