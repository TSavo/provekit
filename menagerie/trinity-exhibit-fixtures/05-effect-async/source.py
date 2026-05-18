# Trinity fixture 05: effect propagation -- async IO
# Exercises: async function definition + await expression (eff_io effect signature)
# The async effect must survive the chain; languages without native async
# emit the effect signature via comment carrier or translate to equivalent blocking form
# with an explicit loss-record entry.

import asyncio


async def fetch_value(key: str) -> int:
    await asyncio.sleep(0)  # yield point; simulate async IO
    # In test harness: replace with real async lookup
    lookup = {"alpha": 1, "beta": 2, "gamma": 3}
    return lookup.get(key, -1)


async def main() -> None:
    result = await fetch_value("beta")
    print(result)  # expected: 2


if __name__ == "__main__":
    asyncio.run(main())
