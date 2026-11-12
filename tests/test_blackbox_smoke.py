"""Black-box smoke suite for a running ShopCore deployment.

Run with: BASE_URL=http://localhost:8000 pytest -q tests/test_blackbox_smoke.py
The suite intentionally uses only HTTP and is independent of Rust internals.
"""

import hashlib
import hmac
import json
import os
import re
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest
import requests

BASE = os.environ.get("BASE_URL", "http://localhost:8000").rstrip("/")
BANK = os.environ.get("BANK_URL", "http://localhost:5001").rstrip("/")
TIMEOUT = float(os.environ.get("HTTP_TIMEOUT", "5"))
NOTIFY_HOST = os.environ.get("NOTIFY_HOST", "host.docker.internal")


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
    response = api("GET", "/api/products?sort=id&limit=100")
    assert response.status_code == 200, response.text
    products = [p for p in response.json() if p.get("stock", 0) >= 5]
    assert products
    return products[0]


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


def test_coupon_discount_and_invalid_coupon():
    user = new_user()
    product = first_product()
    response = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 2}],
        "coupon": "SAVE10",
    })
    assert response.status_code == 201, response.text
    order = response.json()
    assert order["coupon"] == "SAVE10"
    expected = round(product["price"] * 2 * 0.9, 2)
    assert abs(order["total"] - expected) < 0.011

    invalid = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
        "coupon": "NOT-A-COUPON",
    })
    assert invalid.status_code == 400
    assert invalid.json() == {"error": "invalid coupon"}


def test_cancel_restocks_and_rejects_repeat():
    user = new_user()
    product = first_product()
    before = api("GET", f"/api/products/{product['id']}").json()["stock"]
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 2}],
    }).json()
    after_order = api("GET", f"/api/products/{product['id']}").json()["stock"]
    assert after_order == before - 2

    response = api("POST", f"/api/orders/{order['id']}/cancel", user["token"])
    assert response.status_code == 200, response.text
    assert response.json()["status"] == "cancelled"
    assert api("GET", f"/api/products/{product['id']}").json()["stock"] == before

    repeated = api("POST", f"/api/orders/{order['id']}/cancel", user["token"])
    assert repeated.status_code == 409
    assert repeated.json() == {"error": "already cancelled"}


def test_cancel_paid_order_credits_wallet():
    user = new_user()
    product = first_product()
    assert api("POST", "/api/wallet/topup", user["token"], json={"amount": 1000}).status_code == 200
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    paid = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"})
    assert paid.status_code == 200, paid.text
    after_pay = api("GET", "/api/wallet", user["token"]).json()["balance"]

    cancelled = api("POST", f"/api/orders/{order['id']}/cancel", user["token"])
    assert cancelled.status_code == 200, cancelled.text
    assert cancelled.json()["status"] == "cancelled"
    assert api("GET", "/api/wallet", user["token"]).json()["balance"] == 1000
    assert after_pay == 1000 - order["total"]


def test_refund_requires_paid_order_and_credits_amount():
    user = new_user()
    product = first_product()
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    pending = api("POST", f"/api/orders/{order['id']}/refund", user["token"], json={"amount": 1})
    assert pending.status_code == 409
    assert pending.json() == {"error": "order not paid"}

    assert api("POST", "/api/wallet/topup", user["token"], json={"amount": 1000}).status_code == 200
    assert api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"}).status_code == 200
    before = api("GET", "/api/wallet", user["token"]).json()["balance"]

    refunded = api("POST", f"/api/orders/{order['id']}/refund", user["token"], json={"amount": 5})
    assert refunded.status_code == 200, refunded.text
    assert refunded.json()["status"] == "refunded"
    assert refunded.json()["refunded"] == 5
    assert api("GET", "/api/wallet", user["token"]).json()["balance"] == before + 5


def test_wallet_transfer_balances_and_errors():
    source = new_user()
    target = new_user()
    assert api("POST", "/api/wallet/topup", source["token"], json={"amount": 100}).status_code == 200

    response = api("POST", "/api/wallet/transfer", source["token"], json={
        "to_user_id": target["id"], "amount": 30,
    })
    assert response.status_code == 200, response.text
    assert response.json() == {"balance": 70}
    assert api("GET", "/api/wallet", target["token"]).json()["balance"] == 30

    insufficient = api("POST", "/api/wallet/transfer", source["token"], json={
        "to_user_id": target["id"], "amount": 1000,
    })
    assert insufficient.status_code == 402
    assert insufficient.json() == {"error": "insufficient balance"}

    missing = api("POST", "/api/wallet/transfer", source["token"], json={
        "to_user_id": 2147483000, "amount": 1,
    })
    assert missing.status_code == 404
    assert missing.json() == {"error": "recipient not found"}


def test_admin_reports_grouping_and_export():
    admin = admin_token()
    for group in ("day", "status", "user"):
        response = api("GET", f"/api/reports/sales?from=1970-01-01&to=2999-12-31&group_by={group}", admin)
        assert response.status_code == 200, response.text
        body = response.json()
        assert body["from"] == "1970-01-01"
        assert body["to"] == "2999-12-31"
        assert body["group_by"] == group
        assert isinstance(body["buckets"], list)
        assert isinstance(body["top_products"], list)
        for bucket in body["buckets"]:
            assert isinstance(bucket["bucket"], str)
            assert isinstance(bucket["orders"], int)
            assert isinstance(bucket["revenue"], (int, float))

    invalid = api("GET", "/api/reports/sales?from=not-a-date", admin)
    assert invalid.status_code == 400
    assert invalid.json() == {"error": "invalid date"}

    filename = f"charac-{uuid.uuid4().hex}.csv"
    exported = api("POST", "/api/reports/export", admin, json={
        "filename": filename, "from": "1970-01-01", "to": "2999-12-31",
    })
    assert exported.status_code == 200, exported.text
    exported_file = exported.json()["file"]
    assert exported_file.endswith(".gz")

    downloaded = api("GET", "/api/reports/download", admin, params={"file": exported_file})
    assert downloaded.status_code == 200
    assert downloaded.headers["content-type"].startswith("application/gzip")
    assert downloaded.content

    missing = api("GET", "/api/reports/download", admin, params={"file": filename + ".missing"})
    assert missing.status_code == 404
    assert missing.json() == {"error": "not found"}


def test_admin_role_and_order_status_updates():
    admin = admin_token()
    user = new_user()
    role = api("POST", f"/api/admin/users/{user['id']}/role", admin, json={"role": "admin"})
    assert role.status_code == 200, role.text
    assert role.json()["role"] == "admin"

    users = api("GET", "/api/admin/users", admin)
    assert users.status_code == 200
    record = next(u for u in users.json() if u["id"] == user["id"])
    assert record["role"] == "admin"
    assert "password_hash" in record and "created_at" in record

    restored = api("POST", f"/api/admin/users/{user['id']}/role", admin, json={"role": "user"})
    assert restored.status_code == 200
    assert restored.json()["role"] == "user"

    product = first_product()
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    status = api("POST", f"/api/admin/orders/{order['id']}/status", admin, json={"status": "shipped"})
    assert status.status_code == 200, status.text
    assert status.json()["status"] == "shipped"


def test_admin_product_create_update_and_search():
    admin = admin_token()
    sku = "CHARAC-" + uuid.uuid4().hex[:10]
    created = api("POST", "/api/products", admin, json={
        "sku": sku, "name": "Characterization Product", "price": 12.5, "stock": 7,
    })
    assert created.status_code == 201, created.text
    product = created.json()
    assert product["sku"] == sku
    assert product["price"] == 12.5
    assert product["stock"] == 7

    updated = api("PUT", f"/api/products/{product['id']}", admin, json={"price": 13.5, "stock": 9})
    assert updated.status_code == 200, updated.text
    assert updated.json()["price"] == 13.5
    assert updated.json()["stock"] == 9

    found = api("GET", "/api/products?q=Characterization&sort=price&limit=10")
    assert found.status_code == 200
    assert any(item["id"] == product["id"] for item in found.json())


def test_invoice_and_profile_update():
    user = new_user()
    product = first_product()
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 2}],
    }).json()
    invoice = api("GET", f"/api/orders/{order['id']}/invoice", user["token"])
    assert invoice.status_code == 200, invoice.text
    body = invoice.json()
    assert body["order_no"] == order["order_no"]
    assert body["total"] == order["total"]
    assert body["status"] == "pending"
    assert body["customer"]["email"]
    assert len(body["lines"]) == 1
    assert body["lines"][0]["line_total"] == round(product["price"] * 2, 2)

    name = "Renamed " + uuid.uuid4().hex[:6]
    patched = api("PATCH", "/api/users/me", user["token"], json={"name": name})
    assert patched.status_code == 200, patched.text
    assert patched.json()["name"] == name
    assert api("GET", "/api/users/me", user["token"]).json()["name"] == name


def test_error_responses_for_invalid_routes_paths_and_methods_are_json():
    user = new_user()
    for path in ("/api/nope", "/api/products/abc"):
        response = api("GET", path)
        assert response.status_code in (400, 404), response.text
        assert response.headers["content-type"].startswith("application/json")
        assert isinstance(response.json().get("error"), str)

    unauth = api("GET", "/api/orders/abc")
    assert unauth.status_code == 401
    assert unauth.headers["content-type"].startswith("application/json")

    invalid_id = api("GET", "/api/orders/abc", user["token"])
    assert invalid_id.status_code == 404
    assert invalid_id.headers["content-type"].startswith("application/json")
    assert invalid_id.json() == {"error": "not found"}

    wrong_method = api("PUT", "/health")
    assert wrong_method.status_code == 405
    assert wrong_method.headers["content-type"].startswith("application/json")
    assert isinstance(wrong_method.json().get("error"), str)


def test_products_can_sort_by_sku():
    response = api("GET", "/api/products?sort=sku&limit=100")
    assert response.status_code == 200, response.text
    skus = [product["sku"] for product in response.json() if product.get("sku") is not None]
    assert skus == sorted(skus)


def test_pay_method_defaults_to_wallet_and_unknown_is_rejected():
    user = new_user()
    product = first_product()
    assert api("POST", "/api/wallet/topup", user["token"], json={"amount": 1000}).status_code == 200

    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    paid = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={})
    assert paid.status_code == 200, paid.text
    assert paid.json()["method"] == "wallet"

    other = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    unknown = api("POST", f"/api/orders/{other['id']}/pay", user["token"], json={"method": "cash"})
    assert unknown.status_code == 400
    assert unknown.json() == {"error": "unknown method"}
    assert api("GET", f"/api/orders/{other['id']}", user["token"]).json()["status"] == "pending"


class _NotifyHandler(BaseHTTPRequestHandler):
    payloads = []

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length) or b"{}")
        self.__class__.payloads.append(payload)
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, *args):
        pass


def wait_until(predicate, timeout=5.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def test_notify_url_receives_pending_and_paid():
    handler = type("NotifyHandler", (_NotifyHandler,), {"payloads": []})
    server = ThreadingHTTPServer(("0.0.0.0", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        url = f"http://{NOTIFY_HOST}:{server.server_address[1]}/notify"
        user = new_user()
        product = first_product()
        assert api("POST", "/api/wallet/topup", user["token"], json={"amount": 1000}).status_code == 200
        response = api("POST", "/api/orders", user["token"], json={
            "items": [{"product_id": product["id"], "quantity": 1}],
            "notify_url": url,
        })
        assert response.status_code == 201, response.text
        order = response.json()

        assert wait_until(lambda: any(
            payload.get("order_id") == order["id"] and payload.get("status") == "pending"
            for payload in handler.payloads
        )), handler.payloads

        paid = api("POST", f"/api/orders/{order['id']}/pay", user["token"], json={"method": "wallet"})
        assert paid.status_code == 200, paid.text
        assert wait_until(lambda: any(
            payload.get("order_id") == order["id"] and payload.get("status") == "paid"
            for payload in handler.payloads
        )), handler.payloads

        for payload in handler.payloads:
            assert payload["order_no"] == order["order_no"]
            assert set(payload) >= {"order_id", "order_no", "status"}
    finally:
        server.shutdown()
        server.server_close()


def test_order_read_cancel_and_refund_follow_legacy_ownership():
    owner = new_user()
    other = new_user()
    product = first_product()
    order = api("POST", "/api/orders", owner["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()

    fetched = api("GET", f"/api/orders/{order['id']}", other["token"])
    assert fetched.status_code == 200, fetched.text
    assert fetched.json()["id"] == order["id"]

    payment = api("POST", f"/api/orders/{order['id']}/pay", other["token"], json={"method": "wallet"})
    assert payment.status_code == 404

    cancelled = api("POST", f"/api/orders/{order['id']}/cancel", other["token"])
    assert cancelled.status_code == 200, cancelled.text
    assert cancelled.json()["status"] == "cancelled"

    assert api("POST", "/api/wallet/topup", owner["token"], json={"amount": 1000}).status_code == 200
    refundable = api("POST", "/api/orders", owner["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()
    assert api("POST", f"/api/orders/{refundable['id']}/pay", owner["token"], json={"method": "wallet"}).status_code == 200
    before = api("GET", "/api/wallet", owner["token"]).json()["balance"]

    refunded = api("POST", f"/api/orders/{refundable['id']}/refund", other["token"], json={"amount": 5})
    assert refunded.status_code == 200, refunded.text
    assert refunded.json()["status"] == "refunded"
    assert refunded.json()["refunded"] == 5
    assert api("GET", "/api/wallet", owner["token"]).json()["balance"] == before + 5


def test_protected_endpoints_reject_missing_and_invalid_tokens():
    cases = [
        ("GET", "/api/users/me"),
        ("GET", "/api/orders"),
        ("GET", "/api/wallet"),
        ("POST", "/api/logout"),
        ("GET", "/api/reports/sales"),
        ("GET", "/api/admin/users"),
    ]
    for method, path in cases:
        for token in (None, "invalid-token"):
            response = api(method, path, token)
            assert response.status_code == 401, (method, path, token, response.text)
            assert response.headers["content-type"].startswith("application/json")
            assert isinstance(response.json().get("error"), str)


def test_admin_endpoints_reject_non_admin():
    user = new_user()
    cases = [
        ("GET", "/api/admin/users", None),
        ("POST", f"/api/admin/users/{user['id']}/role", {"role": "admin"}),
        ("POST", "/api/admin/orders/1/status", {"status": "shipped"}),
        ("GET", "/api/reports/sales", None),
        ("POST", "/api/reports/export", {"filename": "forbidden.csv"}),
        ("GET", "/api/reports/download?file=missing.gz", None),
    ]
    for method, path, payload in cases:
        if payload is None:
            response = api(method, path, user["token"])
        else:
            response = api(method, path, user["token"], json=payload)
        assert response.status_code == 403, (method, path, response.text)
        assert response.json() == {"error": "forbidden"}


def test_register_and_login_negative_cases():
    base = "neg_" + uuid.uuid4().hex[:8]
    email = base + "@example.com"
    for payload in (
        {},
        {"username": base},
        {"username": base, "email": email},
    ):
        response = api("POST", "/api/register", json=payload)
        assert response.status_code == 400, response.text
        assert isinstance(response.json().get("error"), str)

    assert api("POST", "/api/register", json={
        "username": base, "email": email, "password": "pw12345",
    }).status_code == 201

    duplicate_user = api("POST", "/api/register", json={
        "username": base, "email": "other_" + email, "password": "pw12345",
    })
    assert duplicate_user.status_code == 409

    duplicate_email = api("POST", "/api/register", json={
        "username": base + "_2", "email": email, "password": "pw12345",
    })
    assert duplicate_email.status_code == 409

    assert api("POST", "/api/login", json={"username": base, "password": "wrong"}).status_code == 401
    assert api("POST", "/api/login", json={}).status_code == 401


def test_order_creation_negative_cases():
    user = new_user()
    product = first_product()
    for payload in (
        {"items": []},
        {"items": "not-an-array"},
    ):
        response = api("POST", "/api/orders", user["token"], json=payload)
        assert response.status_code == 400, response.text
        assert isinstance(response.json().get("error"), str)

    unknown_product = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": 99999999, "quantity": 1}],
    })
    assert unknown_product.status_code == 404
    assert unknown_product.json() == {"error": "product 99999999 not found"}

    insufficient = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 999999}],
    })
    assert insufficient.status_code == 409
    assert insufficient.json()["error"].startswith("insufficient stock for ")

    invalid_coupon = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
        "coupon": "NOPE",
    })
    assert invalid_coupon.status_code == 400
    assert invalid_coupon.json() == {"error": "invalid coupon"}

    assert api("GET", "/api/orders/99999999", user["token"]).status_code == 404


def test_pay_negative_cases():
    owner = new_user()
    other = new_user()
    product = first_product()
    order = api("POST", "/api/orders", owner["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()

    insufficient = api("POST", f"/api/orders/{order['id']}/pay", owner["token"], json={"method": "wallet"})
    assert insufficient.status_code == 402
    assert insufficient.json() == {"error": "insufficient wallet balance"}

    unknown = api("POST", f"/api/orders/{order['id']}/pay", owner["token"], json={"method": "cash"})
    assert unknown.status_code == 400
    assert unknown.json() == {"error": "unknown method"}

    cross_user = api("POST", f"/api/orders/{order['id']}/pay", other["token"], json={"method": "wallet"})
    assert cross_user.status_code == 404
    assert cross_user.json() == {"error": "not found"}

    assert api("POST", "/api/wallet/topup", owner["token"], json={"amount": 1000}).status_code == 200
    assert api("POST", f"/api/orders/{order['id']}/pay", owner["token"], json={"method": "wallet"}).status_code == 200
    repeated = api("POST", f"/api/orders/{order['id']}/pay", owner["token"], json={"method": "wallet"})
    assert repeated.status_code == 409
    assert repeated.json() == {"error": "order not payable"}


def test_cancel_and_refund_negative_cases():
    user = new_user()
    product = first_product()
    order = api("POST", "/api/orders", user["token"], json={
        "items": [{"product_id": product["id"], "quantity": 1}],
    }).json()

    pending_refund = api("POST", f"/api/orders/{order['id']}/refund", user["token"], json={"amount": 1})
    assert pending_refund.status_code == 409
    assert pending_refund.json() == {"error": "order not paid"}

    assert api("POST", f"/api/orders/{order['id']}/cancel", user["token"]).status_code == 200
    repeated = api("POST", f"/api/orders/{order['id']}/cancel", user["token"])
    assert repeated.status_code == 409
    assert repeated.json() == {"error": "already cancelled"}

    for action in ("cancel", "refund"):
        response = api("POST", f"/api/orders/99999999/{action}", user["token"], json={})
        assert response.status_code == 404
        assert response.json() == {"error": "not found"}


def test_wallet_transfer_negative_cases():
    source = new_user()
    target = new_user()
    assert api("POST", "/api/wallet/topup", source["token"], json={"amount": 10}).status_code == 200

    insufficient = api("POST", "/api/wallet/transfer", source["token"], json={
        "to_user_id": target["id"], "amount": 1000,
    })
    assert insufficient.status_code == 402
    assert insufficient.json() == {"error": "insufficient balance"}

    missing = api("POST", "/api/wallet/transfer", source["token"], json={
        "to_user_id": 99999999, "amount": 1,
    })
    assert missing.status_code == 404
    assert missing.json() == {"error": "recipient not found"}


def test_admin_negative_targets_and_reports():
    admin = admin_token()
    missing_role = api("POST", "/api/admin/users/99999999/role", admin, json={"role": "admin"})
    assert missing_role.status_code == 404
    assert missing_role.json() == {"error": "not found"}

    missing_status = api("POST", "/api/admin/orders/99999999/status", admin, json={"status": "shipped"})
    assert missing_status.status_code == 404
    assert missing_status.json() == {"error": "not found"}

    invalid_date = api("GET", "/api/reports/sales?from=not-a-date", admin)
    assert invalid_date.status_code == 400
    assert invalid_date.json() == {"error": "invalid date"}

    missing_file = api("GET", "/api/reports/download?file=definitely-missing.gz", admin)
    assert missing_file.status_code == 404
    assert missing_file.json() == {"error": "not found"}


def test_product_and_invoice_negative_cases():
    admin = admin_token()
    user = new_user()

    assert api("GET", "/api/products/99999999").status_code == 404
    missing_update = api("PUT", "/api/products/99999999", admin, json={"price": 1})
    assert missing_update.status_code == 404
    assert missing_update.json() == {"error": "not found"}

    forbidden_create = api("POST", "/api/products", user["token"], json={
        "sku": "NEG-1", "name": "Negative", "price": 1, "stock": 1,
    })
    assert forbidden_create.status_code == 403

    missing_invoice = api("GET", "/api/orders/99999999/invoice", user["token"])
    assert missing_invoice.status_code == 404
    assert missing_invoice.json() == {"error": "not found"}


def test_webhook_negative_signatures():
    body = json.dumps({"order_id": 999999, "bank_ref": "BK-neg", "status": "settled"}, separators=(",", ":")).encode()
    cases = [
        {},
        {"X-Signature": "not-hex"},
        {"X-Signature": "0"},
        {"X-Signature": hmac.new(b"wrong-secret", body, hashlib.sha256).hexdigest()},
    ]
    for headers in cases:
        response = api("POST", "/api/webhooks/bank", data=body, headers={
            "Content-Type": "application/json", **headers,
        })
        assert response.status_code == 401, headers
        assert response.json() == {"error": "bad signature"}


def test_wrong_json_types_and_unknown_methods():
    user = new_user()
    assert api("POST", "/api/register", json={
        "username": 123, "email": "types@example.com", "password": "pw",
    }).status_code == 400
    assert api("POST", "/api/login", json={"username": 123, "password": "pw"}).status_code == 401
    assert api("POST", "/api/orders", user["token"], json={"items": "bad"}).status_code == 400

    wrong_method = api("DELETE", "/api/products")
    assert wrong_method.status_code == 405
    assert wrong_method.headers["content-type"].startswith("application/json")
    assert isinstance(wrong_method.json().get("error"), str)
