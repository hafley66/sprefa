# OpenAPI Cardinality Demo

> **Pets Admin API** version 2026.5


## Operations

### **get** **/pets**

- Operation: **listPets**
- Summary: List pets visible to the current operator
- Response count is the number of rows produced by the nested response query below.

  - **200**: Pet list
  - **401**: Missing or invalid session

### **post** **/pets**

- Operation: **createPet**
- Summary: Create a pet record
- Response count is the number of rows produced by the nested response query below.

  - **201**: Created pet
  - **400**: Invalid pet payload

### **get** **/pets/{petId}**

- Operation: **getPet**
- Summary: Fetch one pet by id
- Response count is the number of rows produced by the nested response query below.

  - **200**: Pet detail
  - **404**: Pet not found


