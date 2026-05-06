# Forward-propagation floor fixture for Ruby
# Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

def checkPositive(x)
  if x <= 0
    return false  # pre: x > 0
  end
  true
end

def callerSatisfiesPre
  result = checkPositive(5)  # satisfies pre (x=5 > 0)
  result
end

def callerViolatesPre
  result = checkPositive(-1)  # violates pre (x=-1 <= 0)
  result
end

def callerWithLoop
  for i in 0...10
    result = checkPositive(i)  # top fallback at loop entry
    return false unless result
  end
  true
end