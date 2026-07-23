Deno.serve(async (request: Request) => {
  const payload = await request.json();
  return Response.json({ received: true, payload });
});
