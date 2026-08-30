#!/usr/bin/env python3
"""Dependency-free S3 copy for the Broccoli contest blob store (SeaweedFS).

Pure Python stdlib only (no boto3/aws-cli) so it runs on an airgapped LAN box in
mainland China with nothing but a system python3. Implements AWS SigV4
(path-style) against the SeaweedFS S3 endpoint.

Subcommands:
  ensure-bucket          create the bucket if it does not exist
  list                   print every object key in the bucket
  dump   --dir DIR       download every object into DIR (key -> relative path)
  load   --dir DIR       upload every file under DIR (relative path -> key)
  count                  print object count

Config via env (or flags): S3_ENDPOINT, S3_BUCKET, S3_REGION,
S3_ACCESS_KEY, S3_SECRET_KEY. Path-style addressing is always used (SeaweedFS).
"""
import sys, os, hashlib, hmac, datetime, argparse, urllib.request, urllib.error, urllib.parse
import xml.etree.ElementTree as ET

EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()


def _cfg(args):
    def g(flag, env):
        return getattr(args, flag, None) or os.environ.get(env)
    endpoint = g("endpoint", "S3_ENDPOINT")
    bucket = g("bucket", "S3_BUCKET")
    region = g("region", "S3_REGION") or "us-east-1"
    ak = g("access_key", "S3_ACCESS_KEY")
    sk = g("secret_key", "S3_SECRET_KEY")
    if not (endpoint and bucket and ak and sk):
        sys.exit("error: need S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY, S3_SECRET_KEY")
    p = urllib.parse.urlparse(endpoint)
    return {
        "scheme": p.scheme or "http",
        "host": p.netloc,            # host:port
        "bucket": bucket, "region": region, "ak": ak, "sk": sk,
    }


def _sign_key(secret, datestamp, region, service):
    k = hmac.new(("AWS4" + secret).encode(), datestamp.encode(), hashlib.sha256).digest()
    k = hmac.new(k, region.encode(), hashlib.sha256).digest()
    k = hmac.new(k, service.encode(), hashlib.sha256).digest()
    k = hmac.new(k, b"aws4_request", hashlib.sha256).digest()
    return k


def _safe_xml(xml_bytes):
    # Dependency-free XXE / billion-laughs guard: the S3 ListObjects response is
    # plain XML with no DTD. Reject any doctype/entity declaration before parsing
    # with the stdlib parser (which is otherwise fine for entity-free documents).
    head = xml_bytes[:4096].lower()
    if b"<!doctype" in head or b"<!entity" in head:
        raise RuntimeError("refusing to parse S3 XML containing a DOCTYPE/ENTITY declaration")
    return ET.fromstring(xml_bytes)


def _request(cfg, method, key="", query=None, body=None, body_file=None,
             body_len=None, payload_sha=None, out_file=None, timeout=180):
    """Signed SigV4 request. body: bytes; body_file: open binary stream to stream."""
    region, service = cfg["region"], "s3"
    now = datetime.datetime.now(datetime.timezone.utc)
    amzdate = now.strftime("%Y%m%dT%H%M%SZ")
    datestamp = now.strftime("%Y%m%d")

    # canonical URI: /bucket[/key]  (encode each path segment, keep '/')
    enc_key = "/".join(urllib.parse.quote(seg, safe="") for seg in key.split("/")) if key else ""
    canonical_uri = "/" + cfg["bucket"] + ("/" + enc_key if enc_key else "")

    q = query or {}
    canonical_qs = "&".join(
        f"{urllib.parse.quote(k, safe='')}={urllib.parse.quote(v, safe='')}"
        for k, v in sorted(q.items()))

    if payload_sha is None:
        if body is not None:
            payload_sha = hashlib.sha256(body).hexdigest()
        elif body_file is not None:
            payload_sha = "UNSIGNED-PAYLOAD"   # stream large blobs without pre-hashing
        else:
            payload_sha = EMPTY_SHA256

    host = cfg["host"]
    canonical_headers = f"host:{host}\nx-amz-content-sha256:{payload_sha}\nx-amz-date:{amzdate}\n"
    signed_headers = "host;x-amz-content-sha256;x-amz-date"
    canonical_request = "\n".join([
        method, canonical_uri, canonical_qs, canonical_headers, signed_headers, payload_sha])

    scope = f"{datestamp}/{region}/{service}/aws4_request"
    string_to_sign = "\n".join([
        "AWS4-HMAC-SHA256", amzdate, scope,
        hashlib.sha256(canonical_request.encode()).hexdigest()])
    signing_key = _sign_key(cfg["sk"], datestamp, region, service)
    signature = hmac.new(signing_key, string_to_sign.encode(), hashlib.sha256).hexdigest()
    authz = (f"AWS4-HMAC-SHA256 Credential={cfg['ak']}/{scope}, "
             f"SignedHeaders={signed_headers}, Signature={signature}")

    url = f"{cfg['scheme']}://{host}{canonical_uri}"
    if canonical_qs:
        url += "?" + canonical_qs
    headers = {"x-amz-date": amzdate, "x-amz-content-sha256": payload_sha, "Authorization": authz}
    data = body
    if body_file is not None:
        data = body_file
        headers["Content-Length"] = str(body_len)
    req = urllib.request.Request(url, data=data, headers=headers, method=method)

    # Retry on transient 5xx/429 (e.g. SeaweedFS auto-growing a volume for a
    # freshly-created bucket returns 500 "please try again" until the volume is
    # ready). Mirrors the broccoli server's own S3 client. Re-seek streamed
    # bodies / output files before each retry so the request is replayable.
    last = None
    for attempt in range(5):
        if attempt:
            import time
            time.sleep(min(0.4 * (2 ** attempt), 5.0))
            if body_file is not None:
                body_file.seek(0)
            if out_file is not None:
                out_file.seek(0); out_file.truncate(0)
        try:
            resp = urllib.request.urlopen(req, timeout=timeout)
            if out_file is not None:
                while True:
                    chunk = resp.read(1 << 20)
                    if not chunk:
                        break
                    out_file.write(chunk)
                return b""
            return resp.read()
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504):
                last = RuntimeError(f"{method} {canonical_uri} -> HTTP {e.code}: {e.read()[:300]!r}")
                continue
            raise RuntimeError(f"{method} {canonical_uri} -> HTTP {e.code}: {e.read()[:300]!r}")
        except (urllib.error.URLError, TimeoutError) as e:
            last = RuntimeError(f"{method} {canonical_uri} -> {e}")
            continue
    raise last


def list_keys(cfg):
    keys, token = [], None
    while True:
        q = {"list-type": "2", "max-keys": "1000"}
        if token:
            q["continuation-token"] = token
        xml = _request(cfg, "GET", query=q)
        root = _safe_xml(xml)
        ns = root.tag[:root.tag.index("}") + 1] if "}" in root.tag else ""
        for c in root.findall(f"{ns}Contents"):
            keys.append(c.find(f"{ns}Key").text)
        trunc = root.find(f"{ns}IsTruncated")
        if trunc is not None and trunc.text == "true":
            nt = root.find(f"{ns}NextContinuationToken")
            token = nt.text if nt is not None else None
            if not token:
                break
        else:
            break
    return keys


def ensure_bucket(cfg):
    try:
        _request(cfg, "GET", query={"list-type": "2", "max-keys": "0"})
        return "exists"
    except RuntimeError:
        _request(cfg, "PUT")   # create bucket
        return "created"


def cmd_dump(cfg, out_dir, keys_file=None):
    if keys_file:
        with open(keys_file) as f:
            keys = [ln.strip() for ln in f if ln.strip()]
    else:
        keys = list_keys(cfg)
    os.makedirs(out_dir, exist_ok=True)
    missing = []
    for i, key in enumerate(keys, 1):
        dest = os.path.join(out_dir, key)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        try:
            with open(dest, "wb") as f:
                _request(cfg, "GET", key=key, out_file=f)
        except RuntimeError as e:
            os.path.exists(dest) and os.remove(dest)
            missing.append(key)
            print(f"  WARN missing blob {key}: {e}", file=sys.stderr)
        if i % 100 == 0 or i == len(keys):
            print(f"  dumped {i}/{len(keys)} blobs", file=sys.stderr)
    if missing:
        print(f"ERROR: {len(missing)} referenced blob(s) missing from S3", file=sys.stderr)
        sys.exit(3)
    print(len(keys))


def cmd_load(cfg, in_dir):
    ensure_bucket(cfg)
    files = []
    for base, _dirs, names in os.walk(in_dir):
        for n in names:
            full = os.path.join(base, n)
            key = os.path.relpath(full, in_dir).replace(os.sep, "/")
            files.append((full, key))
    for i, (full, key) in enumerate(files, 1):
        size = os.path.getsize(full)
        with open(full, "rb") as f:
            _request(cfg, "PUT", key=key, body_file=f, body_len=size)
        if i % 100 == 0 or i == len(files):
            print(f"  loaded {i}/{len(files)} blobs", file=sys.stderr)
    print(len(files))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["ensure-bucket", "list", "dump", "load", "count", "del"])
    ap.add_argument("--dir")
    ap.add_argument("--key", help="object key for 'del'")
    ap.add_argument("--keys-file", dest="keys_file",
                    help="dump only the keys listed in this file (one per line)")
    for f in ("endpoint", "bucket", "region", "access-key", "secret-key"):
        ap.add_argument("--" + f, dest=f.replace("-", "_"))
    args = ap.parse_args()
    cfg = _cfg(args)
    if args.cmd == "ensure-bucket":
        print(ensure_bucket(cfg))
    elif args.cmd == "list":
        for k in list_keys(cfg):
            print(k)
    elif args.cmd == "count":
        print(len(list_keys(cfg)))
    elif args.cmd == "del":
        if not args.key:
            sys.exit("del needs --key")
        _request(cfg, "DELETE", key=args.key)
        print("deleted " + args.key)
    elif args.cmd == "dump":
        if not args.dir:
            sys.exit("dump needs --dir")
        cmd_dump(cfg, args.dir, args.keys_file)
    elif args.cmd == "load":
        if not args.dir:
            sys.exit("load needs --dir")
        cmd_load(cfg, args.dir)


if __name__ == "__main__":
    main()
