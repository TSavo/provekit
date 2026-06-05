from provekit import boundary


@boundary(library="numpy", call="add")
def my_add(x, y):
    raise NotImplementedError
