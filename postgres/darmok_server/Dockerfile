ARG POSTGRES_IMAGE=postgres:18-alpine@sha256:77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873
FROM ${POSTGRES_IMAGE} AS build
USER root
RUN apk add --no-cache gcc make musl-dev clang21 llvm21
COPY . /source
RUN make -C /source && make -C /source install DESTDIR=/installed

FROM ${POSTGRES_IMAGE}
COPY --from=build /installed/ /
# Preloading is explicit at server startup, not baked into cluster data.
CMD ["postgres", "-c", "shared_preload_libraries=darmok_server"]
