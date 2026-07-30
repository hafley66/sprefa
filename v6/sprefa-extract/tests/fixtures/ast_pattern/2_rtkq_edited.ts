declare function createApi(config: unknown): unknown;
declare const api: { injectEndpoints(config: unknown): unknown };
declare const builder: {
  query<Result, Arg>(config: unknown): unknown;
  mutation<Result, Arg>(config: unknown): unknown;
};

export const movedApi = api.injectEndpoints({
  endpoints: (builder) => ({
    updateUser: builder.mutation<User, UserPatch>({ query: (patch) => patch }),
    listUsers: builder.query({ query: () => "/users" }),
  }),
});
