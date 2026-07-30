declare function createApi(config: unknown): unknown;
declare const api: { injectEndpoints(config: unknown): unknown };
declare const builder: {
  query<Result, Arg>(config: unknown): unknown;
  mutation<Result, Arg>(config: unknown): unknown;
};

export const baseApi = createApi({
  reducerPath: "base",
  endpoints: (builder) => ({
    getUser: builder.query<User, string>({ query: (id) => `/users/${id}` }),
    health: builder.query({ query: () => "/health" }),
  }),
});

export const extendedApi = api.injectEndpoints({
  endpoints: (builder) => ({
    createOrder: builder.mutation<Order, NewOrder>({ query: (body) => body }),
  }),
});
