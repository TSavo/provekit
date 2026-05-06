// Forward-propagation floor fixture
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

function checkPositive(x: number): boolean {
  if (x <= 0) { return false; }  // pre: x > 0
  return true;
}

function callerSatisfiesPre() {
  let result = checkPositive(5);  // satisfies pre (x=5 > 0)
  return result;
}

function callerViolatesPre() {
  let result = checkPositive(-1);  // violates pre (x=-1 <= 0)
  return result;
}

function callerWithLoop() {
  for (let i = 0; i < 10; i++) {
    let result = checkPositive(i);  // top fallback at loop entry
    if (!result) { return false; }
  }
  return true;
}