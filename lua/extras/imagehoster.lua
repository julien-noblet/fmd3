function GetImageHosterDirectURL(url)	
	local imghosts = {
		['imagetwist%.com'] = '//img[@class="pic img img-responsive"]/@src',
		['imgchili%.net'] = '//img[@id="show_image"]/@src',
		['imgbox%.com'] = '//img[@id="img"]/@src '
	}
	local u, r, x, k, v = url:lower(), ''
	for k, v in pairs(imghosts) do
		if u:find(k) then
			x = v
			break
		end
	end
	if x and HTTP.GET(url) then
		r = CreateTXQuery(HTTP.Document).x.XPathString(x)
	end
	return r
end
