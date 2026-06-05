# Code under test. The witness pins this file's content (codeCid); mutating it
# breaks the discharge (the witness "is not about this code").
def add(a, b):
    return a + b
