def return_func():
    pass

def other_func():
    pass

def func():
    return return_func

def local_alias():
    result = other_func
    return result

def dict_maker():
    made = {"a": return_func}
    return made

bound = func()
bound()

alias = local_alias
alias()()

made_dict = dict_maker()
made_dict["a"]()
