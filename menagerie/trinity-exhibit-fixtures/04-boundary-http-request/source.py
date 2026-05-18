# Trinity fixture 04: boundary transport -- concept:http-request
# Exercises: concept:http-request (library-bound via provekit-realize-python-requests)
# The concept is preserved via hub CID even though the library binding differs per language.

import urllib.request


def fetch_status(url: str) -> int:
    with urllib.request.urlopen(url) as response:
        return response.status


if __name__ == "__main__":
    # In test harness: replace with local stub server URL
    url = "http://localhost:8080/ping"
    status = fetch_status(url)
    print(status)  # expected: 200
