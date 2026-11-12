"""Black-box smoke suite for a running ShopCore deployment.

Run with: BASE_URL=http://localhost:8000 pytest -q tests/test_blackbox_smoke.py
The suite intentionally uses only HTTP and is independent of Rust internals.
"""

import hashlib
import hmac
import json
import os
import re
import time
import uuid

import pytest
import requests

BASE = os.environ.get("BASE_URL", "http://localhost:8000").rstrip("/")
BANK = os.environ.get("BANK_URL", "http://localhost:5001").rstrip("/")
TIMEOUT = float(os.environ.get("HTTP_TIMEOUT", "5"))


def api(method, path, token=None, **kwargs):
    headers = kwargs.pop("headers", {})
    if token:
        headers["Authorization"] = "Bearer " + token
    return requests.request(method, BASE + path, headers=headers, timeout=TIMEOUT, **kwargs)


def new_user(prefix="u"):
    name = f"{prefix}_{uuid.uuid4().hex[:8]}"
    response = api("POST", "/api/register", json={
        "username": name, "email": name + "@example.com", "password": "pw12345", "name": "Test " + name,
    })
    assert response.status_code == 201, response.text
    user_id = response.json()["id"]
    response = api("POST", "/api/login", json={"username": name, "password": "pw12345"})
    assert response.status_code == 200, response.text
    return {"id": user_id, "token": response.json()["token"]}


def admin_token():
    response = api("POST", "/api/login", json={"username": "admin", "password": "admin123"})
    assert response.status_code == 200, response.text
    return response.json()["token"]


def first_product():
    response = api("GET", "/api/products?sort=id&limit=10")
    assert response.status_code == 200, response.text
    assert response.json()
    return response.json()[0]


@pytest.fixture(scope="session", autouse=True)
def service_is_ready():
    response = requests.get(BASE + "/health", timeout=TIMEOUT)
    assert response.status_code == 200 and response.json().get("status") == "ok"
    bank = requests.get(BANK + "/stats", timeout=TIMEOUT)
    assert bank.status_code == 200


def test_health_and_auth_contract():
    assert api("GET", "/health").json()["status"] == "ok"
    assert api("GET", "/api/users/me").status_code == 401
    user = new_user()
    assert api("GET", "/api/users/me", user["token"]).status_code == 200
    assert api("POST", "/api/logout", user["token"]).status_code == 200


def test_products_and_order_wallet_lifecycle():
    user = new_user()
    product = first_product()
    assert re.fullmatch(r"[A-Z][a-z]{2}, \d{2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}:\d{2} GMT", product["created_at"])
    response = api("POST", "/api/orders", user["token"], json={"items": [{"product_id": product["id"], "quantity": 1}]})
    assert response.status_code == 201, response.text
    order = response.json()
    assert order["status"] == "pending"
    insufficient = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"})
    assert insufficient.status_code == 402
    assert insufficient.json() == {"error": "insufficient wallet balance"}
    assert api("POST", "/api/wallet/topup", user["token"], json={"amount": 500}).status_code == 200
    response = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"})
    assert response.status_code == 200 and response.json()["status"] == "paid"
    repeated = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"})
    assert repeated.status_code == 409
    assert repeated.json() == {"error": "order not payable"}


def test_bank_payment_settles_and_webhook_signature_is_checked():
    user = new_user()
    product = first_product()
    order = api("POST", "/api/orders", user["token"], json={"items": [{"product_id": product["id"], "quantity": 1}]}).json()
    response = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "bank"})
    assert response.status_code == 200 and response.json()["bank_ref"].startswith("BK-")
    deadline = time.time() + 3
    while time.time() < deadline:
        if api("GET", f"/api/orders/{order['id']}", user["token"]).json().get("status") == "paid":
            break
        time.sleep(0.1)
    assert api("GET", f"/api/orders/{order['id']}", user["token"]).json()["status"] == "paid"
    body = json.dumps({"order_id": order["id"], "bank_ref": "BK-manual", "status": "settled"}, separators=(",", ":")).encode()
    signature = hmac.new(os.environ.get("BANK_SECRET", "bank-shared-secret-2019").encode(), body, hashlib.sha256).hexdigest()
    assert api("POST", "/api/webhooks/bank", data=body, headers={"X-Signature": signature, "Content-Type": "application/json"}).status_code == 200
    assert api("POST", "/api/webhooks/bank", data=body, headers={"X-Signature": "deadbeef", "Content-Type": "application/json"}).status_code == 401


def test_admin_boundary_and_bank_idempotency():
    user = new_user()
    assert api("GET", "/api/admin/users", user["token"]).status_code == 403
    key = "smoke-" + uuid.uuid4().hex
    payload = {"order_id": 999, "amount": 1, "idempotency_key": key}
    first = requests.post(BANK + "/charge", json=payload, headers={"Idempotency-Key": key}, timeout=TIMEOUT)
    second = requests.post(BANK + "/charge", json=payload, headers={"Idempotency-Key": key}, timeout=TIMEOUT)
    assert first.status_code == second.status_code == 200
    assert first.json()["bank_ref"] == second.json()["bank_ref"]


def test_invalid_input_keeps_json_error_contract():
    malformed = api("POST", "/api/register", data=b"{", headers={"Content-Type": "application/json"})
    assert malformed.status_code == 400
    assert isinstance(malformed.json().get("error"), str)

    missing = api("POST", "/api/register", json={})
    assert missing.status_code == 400
    assert isinstance(missing.json().get("error"), str)
