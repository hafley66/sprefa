// Mock RTKQ-shaped hook surface for golden_kitchen_sink demo.
import { createApi } from "@reduxjs/toolkit/query/react";

export const usersApi = createApi({
  reducerPath: "usersApi",
  endpoints: (build) => ({
    listUsers: build.query<User[], void>({ query: () => "/users" }),
    getUser: build.query<User, string>({ query: (id) => `/users/${id}` }),
    createUser: build.mutation<User, NewUser>({
      query: (body) => ({ url: "/users", method: "POST", body }),
    }),
    deleteUser: build.mutation<void, string>({
      query: (id) => ({ url: `/users/${id}`, method: "DELETE" }),
    }),
  }),
});

export const {
  useListUsersQuery,
  useGetUserQuery,
  useLazyGetUserQuery,
  useCreateUserMutation,
  useDeleteUserMutation,
} = usersApi;

type User = { id: string; name: string };
type NewUser = { name: string };
