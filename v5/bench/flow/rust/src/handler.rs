// Backend implementor (one of N). Handler fn names are the spec operationIds
// verbatim (the codegen rhythm), so a spec op resolves to this fn cross-repo.
#![allow(non_snake_case)]

pub struct Pet {
    pub id: u64,
}

pub struct CreatedPet {
    pub pet: Pet,
}

// implements operationId: getPet
pub fn getPet(id: u64) -> Pet {
    Pet { id }
}

// implements operationId: createPet
pub fn createPet() -> CreatedPet {
    CreatedPet { pet: getPet(0) }
}
