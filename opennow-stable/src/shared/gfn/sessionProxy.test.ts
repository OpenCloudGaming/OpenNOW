/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  INVALID_SESSION_PROXY_URL_MESSAGE,
  isInvalidSessionProxyUrlError,
  isValidSessionProxyUrl,
  normalizeSessionProxyUrl,
} from "./sessionProxy";

test("normalizes scheme-less host:port values as http proxies", () => {
  assert.equal(normalizeSessionProxyUrl("localhost:8080"), "http://localhost:8080");
  assert.equal(normalizeSessionProxyUrl("proxy.example.com:8080"), "http://proxy.example.com:8080");
  assert.equal(normalizeSessionProxyUrl("127.0.0.1:8080"), "http://127.0.0.1:8080");
});

test("accepts http/https URLs with default ports stripped by WHATWG URL", () => {
  assert.equal(normalizeSessionProxyUrl("http://proxy.example.com:80"), "http://proxy.example.com:80");
  assert.equal(normalizeSessionProxyUrl("https://proxy.example.com:443"), "https://proxy.example.com:443");
  assert.equal(normalizeSessionProxyUrl("http://proxy.example.com"), "http://proxy.example.com:80");
  assert.equal(normalizeSessionProxyUrl("https://proxy.example.com"), "https://proxy.example.com:443");
});

test("accepts supported explicit session proxy schemes with ports", () => {
  assert.equal(normalizeSessionProxyUrl("socks5://proxy.example.com:1080"), "socks5://proxy.example.com:1080");
  assert.equal(normalizeSessionProxyUrl("socks4://proxy.example.com:1080"), "socks4://proxy.example.com:1080");
});

test("rejects socks proxies without an explicit port", () => {
  assert.throws(
    () => normalizeSessionProxyUrl("socks5://proxy.example.com"),
    /Invalid session proxy URL/,
  );
  assert.throws(
    () => normalizeSessionProxyUrl("socks4://proxy.example.com"),
    /Invalid session proxy URL/,
  );
});

test("rejects unsupported schemes and malformed credentials", () => {
  assert.throws(
    () => normalizeSessionProxyUrl("ftp://proxy.example.com:21"),
    /Invalid session proxy URL/,
  );
  assert.throws(
    () => normalizeSessionProxyUrl("http://user%ZZ@proxy.example.com:8080"),
    /Invalid session proxy URL/,
  );
});

test("isValidSessionProxyUrl mirrors normalize semantics", () => {
  assert.equal(isValidSessionProxyUrl(""), true);
  assert.equal(isValidSessionProxyUrl("   "), true);
  assert.equal(isValidSessionProxyUrl("http://proxy.example.com:80"), true);
  assert.equal(isValidSessionProxyUrl("socks5://proxy.example.com"), false);
  assert.equal(isValidSessionProxyUrl("ftp://proxy.example.com:21"), false);
});

test("isInvalidSessionProxyUrlError detects wrapped IPC messages", () => {
  assert.equal(isInvalidSessionProxyUrlError(new Error(INVALID_SESSION_PROXY_URL_MESSAGE)), true);
  assert.equal(
    isInvalidSessionProxyUrlError(
      new Error(`Error invoking remote method 'games:browse-catalog': Error: ${INVALID_SESSION_PROXY_URL_MESSAGE}`),
    ),
    true,
  );
  assert.equal(isInvalidSessionProxyUrlError(new Error("network down")), false);
});
