ARG DARMOK_PRODUCT_IMAGE
FROM ${DARMOK_PRODUCT_IMAGE} AS build
USER root
RUN apk add --no-cache gcc make musl-dev clang21 llvm21
COPY . /source
RUN make -C /source && make -C /source install DESTDIR=/installed

FROM ${DARMOK_PRODUCT_IMAGE}
COPY --from=build /installed/ /
