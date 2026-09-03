def nested_func():
    pass

def param_func(callback):
    callback()

def func(callback):
    callback(nested_func)

def keyword_func(callback):
    callback()

def keyword_target():
    pass

bound_param = param_func
bound_func = func
bound_func(bound_param)

keyword_alias = keyword_func
keyword_value = keyword_target
keyword_alias(callback=keyword_value)
