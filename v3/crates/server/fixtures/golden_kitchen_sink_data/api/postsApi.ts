// Mock RTKQ-shaped hook surface for golden_kitchen_sink demo.
import { createApi } from "@reduxjs/toolkit/query/react";

export const postsApi = createApi({
  reducerPath: "postsApi",
  endpoints: (build) => ({
    listPosts: build.query<Post[], void>({ query: () => "/posts" }),
    getPost: build.query<Post, string>({ query: (id) => `/posts/${id}` }),
    publishPost: build.mutation<Post, NewPost>({
      query: (body) => ({ url: "/posts", method: "POST", body }),
    }),
  }),
});

export const {
  useListPostsQuery,
  useGetPostQuery,
  useLazyGetPostQuery,
  usePublishPostMutation,
} = postsApi;

type Post = { id: string; title: string };
type NewPost = { title: string };
