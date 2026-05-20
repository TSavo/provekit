from provekit import sugar
import requests


@sugar.bind(concept="concept:http-request", library="requests")
def fetch_status(url: str) -> int:
    response = requests.get(url)
    return response.status_code
