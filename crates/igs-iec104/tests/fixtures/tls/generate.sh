#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
#
# Generates the TLS test material of the X1 tests into this directory: a test root CA with a
# bundle of five roots, a server and a client certificate, a client certificate from another CA,
# an expired client certificate, a revoked client certificate with its CRL, a server certificate
# with an RSA-PSS key, and a server certificate above the 8 192-octet limit of clause 6.4.2. The CA keys stay in a temporary
# directory and are not written here.
#
# This is test material only. Never use it outside the tests. Run the script again to regenerate.
# The tests read the files as they are committed. It needs the openssl command, version 3.4 or
# later (for the -not_before and -not_after options of x509, and -startdate of ca).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

# The database and the serial numbers of the test CA.
mkdir ca
: > ca/index.txt
echo 1000 > ca/serial
echo 1000 > ca/crlnumber
cat > ca.cnf <<CONF
[ ca ]
default_ca = test_ca

[ test_ca ]
database = $work/ca/index.txt
serial = $work/ca/serial
crlnumber = $work/ca/crlnumber
certificate = $work/root.pem
private_key = $work/root.key
new_certs_dir = $work/ca
default_md = sha256
default_days = 3650
default_crl_days = 3650
policy = policy_any
copy_extensions = none
unique_subject = no

[ policy_any ]
commonName = supplied
CONF

# A self-signed root certificate, with its key.
root() {
    local name="$1" subject="$2"
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$name.key" 2>/dev/null
    openssl req -new -x509 -key "$name.key" -subj "/CN=$subject" -days 3650 \
        -addext "basicConstraints=critical,CA:TRUE" \
        -addext "keyUsage=critical,keyCertSign,cRLSign" -out "$name.pem"
}

# A leaf certificate of the test root. $3 is the extended key usage, $4 the subject alternative
# names (may be empty). Any further arguments go to `openssl ca` (validity dates).
leaf() {
    local name="$1" subject="$2" usage="$3" names="$4"
    shift 4
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$name.key" 2>/dev/null
    openssl req -new -key "$name.key" -subj "/CN=$subject" -out "$name.csr"
    {
        echo "basicConstraints=CA:FALSE"
        echo "keyUsage=critical,digitalSignature"
        echo "extendedKeyUsage=$usage"
        if [ -n "$names" ]; then echo "subjectAltName=$names"; fi
    } > "$name.ext"
    openssl ca -config ca.cnf -batch -notext -in "$name.csr" -out "$name.pem" \
        -extfile "$name.ext" "$@" 2>/dev/null
}

root root "igs-iec104 test root 1"
for n in 2 3 4 5; do
    root "extra$n" "igs-iec104 test root $n"
done
cat root.pem extra2.pem extra3.pem extra4.pem extra5.pem > roots.pem

leaf server "localhost" serverAuth "DNS:localhost,IP:127.0.0.1" -days 3650
leaf client "igs-client" clientAuth "" -days 3650

# An RSA server certificate: the static RSA and DHE-RSA suites of TLS 1.2 (table 9 of clause 10.5.1)
# need an RSA key, which the EC leaves above do not have.
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out rsa_server.key 2>/dev/null
openssl req -new -key rsa_server.key -subj "/CN=localhost" -out rsa_server.csr
printf "%s\n" "basicConstraints=CA:FALSE" "keyUsage=critical,digitalSignature,keyEncipherment" \
    "extendedKeyUsage=serverAuth" "subjectAltName=DNS:localhost,IP:127.0.0.1" > rsa_server.ext
openssl ca -config ca.cnf -batch -notext -in rsa_server.csr -out rsa_server.pem \
    -extfile rsa_server.ext -days 3650 2>/dev/null

# An RSA-PSS server certificate: its key has the RSA-PSS type, which the signature algorithm
# rsa_pss_pss_sha256 needs (tables 17 and 18 of clause 10.6.2). The key uses SHA-256 and a salt of
# 32 octets.
openssl genpkey -algorithm RSA-PSS -pkeyopt rsa_keygen_bits:2048 -pkeyopt rsa_pss_keygen_md:sha256 \
    -pkeyopt rsa_pss_keygen_mgf1_md:sha256 -pkeyopt rsa_pss_keygen_saltlen:32 \
    -out pss_server.key 2>/dev/null
openssl req -new -key pss_server.key -subj "/CN=localhost" -out pss_server.csr
printf "%s\n" "basicConstraints=CA:FALSE" "keyUsage=critical,digitalSignature" \
    "extendedKeyUsage=serverAuth" "subjectAltName=DNS:localhost,IP:127.0.0.1" > pss_server.ext
openssl ca -config ca.cnf -batch -notext -in pss_server.csr -out pss_server.pem \
    -extfile pss_server.ext -days 3650 2>/dev/null

# A client certificate from a root that the station does not trust.
root rogue "igs-iec104 rogue root"
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out rogue_client.key 2>/dev/null
openssl req -new -key rogue_client.key -subj "/CN=rogue-client" -out rogue_client.csr
printf '%s\n' "basicConstraints=CA:FALSE" "keyUsage=critical,digitalSignature" \
    "extendedKeyUsage=clientAuth" > rogue_client.ext
openssl x509 -req -in rogue_client.csr -CA rogue.pem -CAkey rogue.key -CAcreateserial \
    -days 3650 -extfile rogue_client.ext -out rogue_client.pem 2>/dev/null

# A client certificate that expired in 2020.
leaf expired_client "expired-client" clientAuth "" \
    -startdate 20200101000000Z -enddate 20200102000000Z

# A client certificate, revoked by the test CA, and the CRL that lists it.
leaf revoked_client "revoked-client" clientAuth "" -days 3650
openssl ca -config ca.cnf -revoke revoked_client.pem 2>/dev/null
openssl ca -config ca.cnf -gencrl -out crl.pem 2>/dev/null

# A server certificate with so many names that its DER is above 8 192 octets.
names=""
for i in $(seq -w 1 400); do
    names="$names,DNS:station-$i.grid.example.test"
done
leaf big_server "big-server" serverAuth "${names#,}" -days 3650
size="$(openssl x509 -in big_server.pem -outform DER | wc -c)"
if [ "$size" -le 8192 ]; then
    echo "big_server.pem is $size octets, not above 8192" >&2
    exit 1
fi

# The files that the tests read. Each one is checked against the test root before it is copied.
openssl verify -CAfile roots.pem server.pem client.pem big_server.pem rsa_server.pem \
    pss_server.pem >/dev/null
for name in server client big_server rsa_server pss_server revoked_client expired_client rogue_client; do
    cp "$name.pem" "$here/"
done
for name in server client big_server rsa_server pss_server rogue_client expired_client revoked_client; do
    cp "$name.key" "$here/"
done
cp roots.pem crl.pem "$here/"
echo "wrote the TLS test material to $here (big_server.pem: $size octets)"
