// React components consuming RTK Query auto-generated hooks. RTKQ derives the
// hook name from the endpoint: endpoint `getUser` -> `useGetUserQuery`, a
// mutation `createOrder` -> `useCreateOrderMutation`, and a lazy variant
// `useLazyGetUserQuery`. The hook identifier is NOT the operationId verbatim,
// so cross-linking back to the spec needs the affix recovery in
// rtkq-op-recovery.dl.

function UserCard({ id }: { id: string }) {
  const { data } = useGetUserQuery(id);          // getUser
  return data;
}

function UserList() {
  const { data } = useListUsersQuery();          // listUsers
  return data;
}

function NewOrderButton() {
  const [createOrder] = useCreateOrderMutation(); // createOrder
  return createOrder;
}

function PrefetchedUser() {
  const [trigger] = useLazyGetUserQuery();        // getUser (Lazy variant)
  return trigger;
}

// Stale hook: the `deleteWidget` endpoint was removed from the spec but the
// component still calls its generated hook. Recovery + the spec join flags it.
function WidgetRow() {
  const [del] = useDeleteWidgetMutation();        // deleteWidget -> orphan
  return del;
}
