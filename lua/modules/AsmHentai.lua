function Init()
	local m = NewWebsiteModule()
	m.ID                       = '845f86020f8b4744824f5eaffc1f278f'
	m.Name                     = 'AsmHentai'
	m.RootURL                  = 'https://asmhentai.com'
	m.Category                 = 'H-Sites'
	m.OnGetDirectoryPageNumber = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.SortedList               = true
end

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL) then
		local x = CreateTXQuery(HTTP.Document)
		PAGENUMBER = tonumber(x.XPathString('//*[@class="pagination"]/a[last()-1]')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/pag/' .. (URL + 1) .. '/') then
		local x = CreateTXQuery(HTTP.Document)
		x.XPathHREFAll('//*[@class="preview_item"]/*[@class="caption"]/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Title     = x.XPathString('//div[@class="info"]/h1')
		MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL, x.XPathString('//div[@class="cover"]//img/@src'))
		MANGAINFO.Artists   = x.XPathStringAll('//div[@class="tags" and contains(h3, "Artist")]/div/a/span/text()')
		MANGAINFO.Genres    = x.XPathStringAll('//div[@class="tags" and (contains(h3, "Tags") or contains(h3, "Category"))]/div/a/span/text()')

		MANGAINFO.ChapterLinks.Add(MANGAINFO.URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x = CreateTXQuery(HTTP.Document)
		local id        = x.XPathString('//*[@id="dir"]/@value')
		local dir       = x.XPathString('//*[@id="id"]/@value')
		TASK.PageNumber = tonumber(x.XPathString('//*[@id="t_pages"]/@value'))
		for i = 1, TASK.PageNumber do
			TASK.PageLinks.Add('https://images.asmhentai.com/' .. id .. '/' .. dir .. '/' .. i .. '.jpg')
		end
		return true
	else
		return false
	end
end